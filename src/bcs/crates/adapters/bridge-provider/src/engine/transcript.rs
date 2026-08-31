//! Transcript sink for engine-native session files.
//!
//! A [`TranscriptSink`] writes an inject message into the engine's own
//! per-session transcript file so the message lives in the engine's history
//! and is visible to future turns without us driving a turn (spec §5.1:
//! inject never triggers an engine run). The CC engine's transcript lives at
//! `~/.claude/projects/<encoded-cwd>/<engine_session_id>.jsonl` (cwd `/`→`-`),
//! and is read back at `cfuse --cc --resume <engine_session_id>` — so a sunk
//! `user` entry fortifies the conversation with the inject text.
//!
//! [`ClaudeJsonlSink`] is the CC sink. Codex is sunk as `None` (no sink): its
//! injects stay in `pending_injects` and are prepended to the next chat.send
//! prompt as `[from:{name}] {text}` (see `run::assemble_prompt`).
//!
//! Idempotency: each appended entry carries `bridgeInjectId = inject run_id`.
//! Before appending, we scan the file's existing content for a line bearing
//! the same `bridgeInjectId` and skip if present — a retry with the same id
//! does not duplicate the entry.
//!
//! Chain link: a new entry's `parentUuid` is set to the last existing line's
//! `uuid` if one is present (best-effort single-line lookback), matching the
//! cc JSONL convention; if the file is missing or no predecessor is parseable
//! the field is omitted.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::session::InjectedMessage;

/// Append an inject message into an engine-native per-session transcript file.
/// Implementations are idempotent on the inject's `run_id`: a second call with
/// the same `run_id` MUST be a no-op.
pub trait TranscriptSink: Send + Sync {
    fn append_user_message(
        &self,
        cwd: &Path,
        engine_session_id: &str,
        msg: &InjectedMessage,
    ) -> Result<(), TranscriptError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("transcript io: {0}")]
    Io(#[from] std::io::Error),
    #[error("transcript serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// CC transcript sink: writes user entries into
/// `<projects_root>/<encoded-cwd>/<engine_session_id>.jsonl`. Production
/// resolves `<projects_root>` from `$HOME/.claude/projects`; tests inject a
/// tempdir root via [`ClaudeJsonlSink::with_projects_root`].
pub struct ClaudeJsonlSink {
    projects_root: PathBuf,
}

impl ClaudeJsonlSink {
    /// Test/dev constructor: place transcripts under `root`.
    pub fn with_projects_root(root: PathBuf) -> Self {
        Self { projects_root: root }
    }

    /// Production constructor: `$HOME/.claude/projects`. Returns `None` when
    /// `$HOME` is unset (the caller MUST then fall back to pending injects —
    /// there is no transcript file we can locate).
    pub fn default_home() -> Option<Self> {
        std::env::var_os("HOME").map(|h| Self {
            projects_root: PathBuf::from(h).join(".claude").join("projects"),
        })
    }

    /// Per-session transcript path: `<projects_root>/<encoded-cwd>/<engine_session_id>.jsonl`.
    fn session_file(&self, cwd: &Path, engine_session_id: &str) -> PathBuf {
        let encoded = encode_cwd(cwd);
        self.projects_root
            .join(encoded)
            .join(format!("{engine_session_id}.jsonl"))
    }
}

impl TranscriptSink for ClaudeJsonlSink {
    fn append_user_message(
        &self,
        cwd: &Path,
        engine_session_id: &str,
        msg: &InjectedMessage,
    ) -> Result<(), TranscriptError> {
        let file = self.session_file(cwd, engine_session_id);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Read existing content; missing file is treated as empty (new session).
        let existing = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(TranscriptError::Io(e)),
        };
        // Idempotency: skip if an entry with the same `bridgeInjectId` (= run_id)
        // already lives in the file. Substring scan is safe — run_ids are unique
        // and the literal `"bridgeInjectId":"<run_id>"` shape is fixed by us.
        let needle = format!("\"bridgeInjectId\":\"{}\"", msg.run_id);
        if existing.contains(&needle) {
            return Ok(());
        }
        // Best-effort `parentUuid`: last non-empty line's `uuid` if present.
        let parent_uuid = existing
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .and_then(|l| serde_json::from_str::<Value>(l).ok())
            .and_then(|v| v.get("uuid").and_then(|x| x.as_str()).map(str::to_string));

        // Entry text: `[from:{name}] {text}` (or bare `{text}` when no name).
        let text = match &msg.from_name {
            Some(name) => format!("[from:{name}] {}", msg.text),
            None => msg.text.clone(),
        };
        let mut entry = json!({
            "type": "user",
            "uuid": uuid::Uuid::new_v4().to_string(),
            "sessionId": engine_session_id,
            "bridgeInjectId": msg.run_id,
            "timestamp": bcs_protocol::now_ms(),
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": text }]
            }
        });
        if let Some(p) = parent_uuid {
            entry["parentUuid"] = Value::String(p);
        }
        let line = serde_json::to_string(&entry)? + "\n";
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }
}

/// Encode a cwd path for the projects-dir layout: every `/` becomes `-`.
/// `/tmp/work` → `-tmp-work`; the leading slash also maps to `-`.
fn encode_cwd(cwd: &Path) -> String {
    cwd.to_string_lossy().replace('/', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::InjectedMessage;
    use std::path::Path;

    #[test]
    fn claude_jsonl_sink_appends_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        // Claude project layout: <root>/<encoded-cwd>/<session>.jsonl;
        // encoded-cwd = path '/'->'-'
        let projects = dir.path().join("projects");
        let sess_dir = projects.join("-tmp-work");
        std::fs::create_dir_all(&sess_dir).unwrap();
        let sess_file = sess_dir.join("sess-1.jsonl");
        std::fs::write(&sess_file, "{\"type\":\"assistant\",\"uuid\":\"u1\",\"message\":{}}\n").unwrap();

        let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
        let msg = InjectedMessage { run_id: "inj-1".into(), from_name: Some("张三".into()), text: "观察".into() };
        sink.append_user_message(Path::new("/tmp/work"), "sess-1", &msg).unwrap();
        sink.append_user_message(Path::new("/tmp/work"), "sess-1", &msg).unwrap(); // idempotent

        let content = std::fs::read_to_string(&sess_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2); // only one new line appended
        let appended: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(appended["type"], serde_json::json!("user"));
        assert_eq!(appended["parentUuid"], serde_json::json!("u1"));
        assert_eq!(appended["bridgeInjectId"], serde_json::json!("inj-1"));
        assert_eq!(appended["message"]["content"][0]["text"], serde_json::json!("[from:张三] 观察"));
    }

    #[test]
    fn encode_cwd_replaces_slashes_with_dashes() {
        assert_eq!(encode_cwd(Path::new("/tmp/work")), "-tmp-work");
        assert_eq!(encode_cwd(Path::new("/")), "-");
        assert_eq!(encode_cwd(Path::new("relative/nested")), "relative-nested");
    }

    #[test]
    fn sink_creates_missing_session_dir_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
        let msg = InjectedMessage { run_id: "inj-2".into(), from_name: None, text: "bare text".into() };
        // No pre-existing dir/file; sink must create both.
        sink.append_user_message(Path::new("/tmp/other"), "sess-new", &msg).unwrap();
        let file = projects.join("-tmp-other").join("sess-new.jsonl");
        let content = std::fs::read_to_string(&file).unwrap();
        let appended: Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(appended["type"], serde_json::json!("user"));
        assert_eq!(appended["message"]["content"][0]["text"], serde_json::json!("bare text"));
        assert_eq!(appended["bridgeInjectId"], serde_json::json!("inj-2"));
        assert!(appended.get("parentUuid").is_none(), "no parent for a fresh file");
    }

    #[test]
    fn sink_prepends_from_name_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
        let msg = InjectedMessage {
            run_id: "inj-3".into(),
            from_name: Some("李四".into()),
            text: "hello".into(),
        };
        sink.append_user_message(Path::new("/tmp/x"), "sess-x", &msg).unwrap();
        let file = projects.join("-tmp-x").join("sess-x.jsonl");
        let content = std::fs::read_to_string(&file).unwrap();
        let appended: Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(
            appended["message"]["content"][0]["text"],
            serde_json::json!("[from:李四] hello")
        );
    }

    #[test]
    fn sink_chain_links_parent_uuid_to_last_line() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        let sess_dir = projects.join("-tmp-chain");
        std::fs::create_dir_all(&sess_dir).unwrap();
        let sess_file = sess_dir.join("c.jsonl");
        // Seed two prior lines; the sink must pick up the LAST line's uuid.
        std::fs::write(
            &sess_file,
            "{\"type\":\"user\",\"uuid\":\"a1\",\"message\":{}}\n\
             {\"type\":\"assistant\",\"uuid\":\"a2\",\"message\":{}}\n",
        )
        .unwrap();
        let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
        let msg = InjectedMessage { run_id: "inj-c".into(), from_name: None, text: "c".into() };
        sink.append_user_message(Path::new("/tmp/chain"), "c", &msg).unwrap();
        let content = std::fs::read_to_string(&sess_file).unwrap();
        let appended: Value =
            serde_json::from_str(content.lines().last().unwrap()).unwrap();
        assert_eq!(appended["parentUuid"], serde_json::json!("a2"));
    }

    #[test]
    fn sink_distinct_run_ids_append_distinct_lines() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
        let m1 = InjectedMessage { run_id: "inj-a".into(), from_name: None, text: "one".into() };
        let m2 = InjectedMessage { run_id: "inj-b".into(), from_name: None, text: "two".into() };
        sink.append_user_message(Path::new("/tmp/d"), "s", &m1).unwrap();
        sink.append_user_message(Path::new("/tmp/d"), "s", &m2).unwrap();
        let file = projects.join("-tmp-d").join("s.jsonl");
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content.lines().count(), 2);
        // Re-appending the FIRST run_id still no-ops (idempotency is per-run_id).
        sink.append_user_message(Path::new("/tmp/d"), "s", &m1).unwrap();
        let content2 = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content2.lines().count(), 2, "idempotent per-run_id: re-append no-ops");
    }
}
