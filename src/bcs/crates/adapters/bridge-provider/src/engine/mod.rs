pub mod cfuse_cc;
pub mod cfuse_codex;
pub mod cli;
pub mod transcript;

use std::path::PathBuf;
use std::sync::Arc;

use bcs_protocol::stream::StreamEvent;

pub use crate::config::EngineKind;
use crate::config::BotConfig;
use crate::interaction::InteractionRegistry;

/// One BCS downstream turn request.
///
/// `run_id` is the BCS downstream body id; frames use it as `runId`.
/// `engine_session_id` carries the engine-native session id on follow-up turns.
/// `cfuse_bin` is the resolved engine binary path.
/// `interactions` is the run's HITL interaction registry: the cc driver
/// registers pending `can_use_tool`/`AskUserQuestion` requests with it, the
/// webhook `interaction.resolve` handler delivers decisions to it.
pub struct TurnRequest {
    pub run_id: String,
    pub prompt: String,
    pub engine_session_id: Option<String>,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub cfuse_bin: PathBuf,
    pub permission_mode: Option<String>,
    pub interactions: InteractionRegistry,
}

/// Outcome of an engine turn: the engine-internal session id (if one was
/// established/resumed) and the final assistant text, if the turn completed.
#[derive(Debug)]
pub struct TurnOutcome {
    pub engine_session_id: Option<String>,
    pub final_text: Option<String>,
}

/// Engine turn errors. `EngineExited` carries the engine's own exit reason;
/// `Aborted` is returned when the run is cancelled via the abort token.
/// `Io` is raised on stdout/stdin IO failures (e.g. broken pipe) and converts
/// from [`std::io::Error`] via `?` for the driver's read/write paths.
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    #[error("spawn engine: {0}")]
    Spawn(std::io::Error),
    #[error("engine exited: {0}")]
    EngineExited(String),
    #[error("engine turn aborted")]
    Aborted,
    #[error("engine protocol error: {0}")]
    Protocol(String),
    #[error("engine io: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    fn kind(&self) -> EngineKind;
    async fn run_turn(
        &self,
        req: TurnRequest,
        events: tokio::sync::mpsc::Sender<StreamEvent>,
        abort: tokio_util::sync::CancellationToken,
    ) -> Result<TurnOutcome, TurnError>;
}

/// Build an [`Engine`] for `bot`. Both `CfuseCc` and `CfuseCodex` are wired
/// to their real drivers ([`cfuse_cc::CfuseCc`] / [`cfuse_codex::CfuseCodex`]).
pub fn build_engine(bot: &BotConfig) -> Arc<dyn Engine> {
    match bot.engine {
        EngineKind::CfuseCc => Arc::new(cfuse_cc::CfuseCc::new(
            bot.cfuse_bin.clone().unwrap_or_else(|| PathBuf::from("cfuse")),
        )),
        EngineKind::CfuseCodex => Arc::new(cfuse_codex::CfuseCodex::new(
            bot.cfuse_bin.clone().unwrap_or_else(|| PathBuf::from("cfuse")),
        )),
    }
}

/// Validate an engine-native session id before it is used as a transcript path
/// component (`<engine_session_id>.jsonl`) or a `--resume`/`exec resume` argv
/// argument. An engine must never be a trusted source for these — a buggy or
/// hostile engine could supply `../../evil` (path traversal) or `--evil`
/// (argv option injection). Rules: non-empty; no leading dash (argv option
/// guard); no path separators or parent refs; only ascii alphanumeric plus
/// `-`/`_`/`.`. The two engine drivers call this at their capture sites (cc
/// `system/init`, codex `thread.started`); an invalid id is logged and treated
/// as no session (not persisted, not resumed, transcript sink skipped).
pub(crate) fn is_valid_engine_session_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.contains(['/', '\\'])
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcs_protocol::stream::StreamEvent;

    struct FakeEngine;
    #[async_trait::async_trait]
    impl Engine for FakeEngine {
        fn kind(&self) -> EngineKind { EngineKind::CfuseCc }
        async fn run_turn(&self, req: TurnRequest,
                          events: tokio::sync::mpsc::Sender<StreamEvent>,
                          _abort: tokio_util::sync::CancellationToken)
            -> Result<TurnOutcome, TurnError> {
            let _ = events.send(crate::sse::chat_delta(&req.run_id, "fake")).await;
            Ok(TurnOutcome { engine_session_id: Some("e-1".into()), final_text: Some("done".into()) })
        }
    }

    #[tokio::test]
    async fn fake_engine_emits_delta() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let engine = FakeEngine;
        let req = TurnRequest {
            run_id: "r-1".into(), prompt: "hi".into(), engine_session_id: None,
            cwd: ".".into(), model: None, cfuse_bin: "cfuse".into(), permission_mode: None,
            interactions: InteractionRegistry::new(),
        };
        let outcome = engine.run_turn(req, tx, tokio_util::sync::CancellationToken::new()).await.unwrap();
        assert_eq!(outcome.engine_session_id.as_deref(), Some("e-1"));
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn build_engine_wires_cfuse_codex_driver() {
        // 不 spawn：只确认 build_engine 对 CfuseCodex 返回真实驱动（而非 stub）。
        let bot = BotConfig {
            provider_bot_ref: "codex-worker".into(),
            engine: EngineKind::CfuseCodex,
            model: None,
            cwd: "/tmp".into(),
            permission_mode: None,
            cfuse_bin: Some(PathBuf::from("/usr/local/bin/cfuse")),
        };
        let engine = build_engine(&bot);
        assert_eq!(engine.kind(), EngineKind::CfuseCodex);
    }

    #[tokio::test]
    async fn build_engine_wires_cfuse_cc_driver() {
        // 不 spawn：只确认 build_engine 对 CfuseCc 返回真实驱动（而非 stub）。
        let bot = BotConfig {
            provider_bot_ref: "cc-worker".into(),
            engine: EngineKind::CfuseCc,
            model: None,
            cwd: "/tmp".into(),
            permission_mode: None,
            cfuse_bin: Some(PathBuf::from("/usr/local/bin/cfuse")),
        };
        let engine = build_engine(&bot);
        assert_eq!(engine.kind(), EngineKind::CfuseCc);
    }

    #[test]
    fn is_valid_engine_session_id_accepts_safe_ids() {
        assert!(is_valid_engine_session_id("cc-sess-1"));
        assert!(is_valid_engine_session_id("01a058a5-5bb3-7702-bc4c-7d26b3bfa32d"));
        // underscores and dots are in the allowed set; a single dot is fine.
        assert!(is_valid_engine_session_id("thread_42"));
        assert!(is_valid_engine_session_id("sess.1"));
    }

    #[test]
    fn is_valid_engine_session_id_rejects_unsafe_ids() {
        // empty
        assert!(!is_valid_engine_session_id(""), "empty rejected");
        // path separators (path traversal)
        assert!(!is_valid_engine_session_id("a/b"), "forward slash rejected");
        assert!(!is_valid_engine_session_id("a\\b"), "backslash rejected");
        assert!(!is_valid_engine_session_id("../x"), "parent ref rejected");
        assert!(!is_valid_engine_session_id("a..b"), "embedded parent ref rejected");
        // leading dash (argv option injection)
        assert!(!is_valid_engine_session_id("--evil"), "leading dash rejected");
        // whitespace / other disallowed chars
        assert!(!is_valid_engine_session_id("a b"), "space rejected");
        assert!(!is_valid_engine_session_id("a:b"), "colon rejected");
        assert!(!is_valid_engine_session_id("café"), "non-ascii rejected");
    }
}
