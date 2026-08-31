use axum::http::StatusCode;
use futures::StreamExt;
use serde_json::json;
use std::path::Path;

mod support; // tests/support/mod.rs：spawn_app(config_toml: &str) -> String(base_url)

/// Serialize tests that mutate the process-global `HOME` env var. The cc
/// transcript sink reads `~/.claude/projects` from `$HOME`, and
/// `std::env::set_var` mutates the global env — pairing this lock with the
/// [`HomeGuard`] RAII below keeps HOME-mutating tests out of each other's way.
/// `std::sync::Mutex` (const constructor; `!Send` guard is fine because the
/// `#[tokio::test]` runtime is current-thread and no other task takes this
/// lock — only HOME-mutating tests touch it, and they all acquire it).
static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard that restores the original `HOME` on drop. Pair with the
/// [`HOME_LOCK`] mutex to isolate tests that need the cc transcript sink to
/// resolve a tempdir as `$HOME/.claude/projects`.
struct HomeGuard(Option<std::ffi::OsString>);
#[allow(unsafe_code)] // env mutation is unsafe on edition 2024; HOME_LOCK serializes access
impl HomeGuard {
    fn set(dir: &Path) -> Self {
        let prev = std::env::var_os("HOME");
        // SAFETY: the caller holds HOME_LOCK for the duration of the test, so
        // no other code in this process mutates/reads HOME concurrently.
        unsafe { std::env::set_var("HOME", dir) };
        Self(prev)
    }
}
#[allow(unsafe_code)] // see impl HomeGuard above — same HOME_LOCK exclusivity
impl Drop for HomeGuard {
    fn drop(&mut self) {
        // SAFETY: same HOME_LOCK exclusivity protects the restore path.
        match self.0.take() {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

#[tokio::test]
async fn ping_requires_auth_and_matching_provider() {
    let url = support::spawn_app(r#"
provider_id = "bridge-1"
listen = "127.0.0.1:0"
bcs_to_provider_token = "tok-b2p"
[[bot]]
provider_bot_ref = "cc-worker"
engine = "cfuse-cc"
cwd = "/tmp"
"#).await;
    let client = reqwest::Client::new();

    // 无 token → 401
    let resp = client.post(format!("{url}/webhook"))
        .json(&json!({"type":"req","id":"1","method":"bot.ping",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], json!("unauthorized"));

    // provider_id 不匹配 → 403
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"2","method":"bot.ping",
                      "to_bot":{"provider_id":"other","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 未知 method → 501
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"3","method":"chat.explode",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);

    // ping 正常 → 200
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"4","method":"bot.ping",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));
}

#[tokio::test]
async fn chat_send_streams_sse_to_final() {
    // mock_cc.sh：读一行 stdin 后回放 cc_turn.ndjson，完整跑一轮到 result/success。
    let url = support::spawn_app_with_mock("mock_cc.sh", "cfuse-cc").await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&json!({
            "type": "req", "id": "run-1", "method": "chat.send",
            "session_id": "s-1",
            "to_bot": {"provider_id": "bridge-1", "provider_bot_ref": "worker-1"},
            "message": {"role": "user", "content": [{"type": "text", "text": "你好"}]}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let text = resp.text().await.unwrap();
    assert!(text.contains("event: agent"), "missing agent tool event: {text}");
    assert!(text.contains("\"state\":\"delta\""), "missing delta state: {text}");
    assert!(text.contains("\"deltaText\":\"正在\""), "missing delta text: {text}");
    assert!(text.contains("\"state\":\"final\""), "missing final state: {text}");
    assert!(text.contains("完成了"), "missing final assistant text: {text}");
    // seq 单调递增
    let seqs = support::extract_seqs(&text);
    assert!(!seqs.is_empty(), "no seq frames: {text}");
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seq not monotonic: {seqs:?}");
}

#[tokio::test]
async fn concurrent_send_same_session_gets_429() {
    // mock_cc_slow.sh：读 stdin 后 sleep 30 再吐结果，保证第一个 run 仍在执行。
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let body = |id: &str| serde_json::json!({
        "type": "req", "id": id, "method": "chat.send",
        "session_id": "s-1",
        "to_bot": {"provider_id": "bridge-1", "provider_bot_ref": "worker-1"},
        "message": {"role": "user", "content": [{"type": "text", "text": "hi"}]}
    });

    let first = client
        .post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&body("run-a"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK, "first run SSE stream must start");

    let second = client
        .post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&body("run-b"))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    let err: serde_json::Value = second.json().await.unwrap();
    assert_eq!(err["error"]["code"], serde_json::json!("rate_limited"));
    assert_eq!(err["error"]["retryable"], serde_json::json!(true));
}

#[tokio::test]
async fn interaction_roundtrip_over_sse_and_resolve_webhook() {
    // mock_cc_approval.sh：读 user 消息后吐 control_request(can_use_tool Bash)，
    // 等待 stdin 的 control_response，按 behavior 吐 result。
    let url = support::spawn_app_with_mock("mock_cc_approval.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&json!({"type":"req","id":"run-1","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"执行一下"}]}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let mut stream = resp.bytes_stream();
    let mut acc = String::new();
    // 读到 interaction/requested 帧为止
    let iid = loop {
        let chunk = stream.next().await.unwrap().unwrap();
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("\"phase\":\"requested\"") {
            break support::extract_first_interaction_id(&acc);
        }
    };
    // BCS 回程：interaction.resolve
    let ack = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"resolve-1","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":iid,
                      "kind":"exec","idempotencyKey":"key-1","decision":"allow_once"}}))
        .send().await.unwrap();
    assert_eq!(ack.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));
    // 幂等重放同 key
    let dup = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"resolve-2","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":iid,
                      "kind":"exec","idempotencyKey":"key-1","decision":"allow_once"}}))
        .send().await.unwrap();
    assert_eq!(dup.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));
    // 未知 interactionId → 字符串形态 error
    let unknown = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"resolve-3","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":"int-nope",
                      "kind":"exec","idempotencyKey":"key-9","decision":"deny"}}))
        .send().await.unwrap();
    let body: serde_json::Value = unknown.json().await.unwrap();
    assert_eq!(body["ok"], json!(false));
    assert!(body["error"].is_string());  // 注意：此方法的 error 是字符串（spec §5.1）
    // 流继续：resolved → chat/final
    while let Some(chunk) = stream.next().await {
        acc.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        if acc.contains("\"state\":\"final\"") { break; }
    }
    assert!(acc.contains("\"phase\":\"resolved\""));
    assert!(acc.contains("\"state\":\"final\""));
}

#[tokio::test]
async fn inject_then_send_prepends_for_codex() {
    // mock_codex.sh emits codex JSONL echoing each prompt line as agent_message
    // deltas, so the chat.send SSE body carries the assembled prompt text.
    let url = support::spawn_app_with_mock("mock_codex.sh", "cfuse-codex").await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"inj-1","method":"chat.inject",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"观察上下文"}]},
            "from":{"kind":"bot","name":"观察者"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));

    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&json!({"type":"req","id":"run-9","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"正式问题"}]}}))
        .send().await.unwrap();
    let text = resp.text().await.unwrap();
    // The prefix from the injected message is prepended with the `[from:{name}]`
    // envelope, and it must precede the current `正式问题` message body in the
    // assembled prompt. mock_codex.sh echoes each non-blank prompt line as its
    // own `agent_message` text inside a JSONL frame that the codex driver maps
    // to a chat_delta SSE event — so both needles appear literally in the SSE
    // stream text (no JSON-escaping of these ASCII-bracket/multi-byte chars).
    assert!(text.contains("[from:观察者] 观察上下文"), "inject prefix missing: {text}");
    let pos_inject = text.find("[from:观察者] 观察上下文")
        .expect("inject prefix position");
    let pos_main = text.find("正式问题").expect("main message position");
    assert!(pos_inject < pos_main, "inject must precede the current message: {text}");
}

#[tokio::test]
async fn inject_sinks_to_cc_transcript_and_does_not_pending() {
    // cc sink-success branch: with an established `engine_session_id`, a cc
    // bot's inject must land in the engine transcript file (and NOT also be
    // added to `pending_injects` — that would double-deliver on the next
    // chat.send). Idempotent replay keeps the file at exactly one entry.
    //
    // Serialize HOME-mutating tests (set_var/restore) — `cargo test` runs tests
    // in parallel across OS threads within the same process, so env mutation
    // is process-global. The cc transcript sink resolves `~/.claude/projects`
    // from `$HOME`. Only HOME-mutating tests acquire HOME_LOCK, and no other
    // test reads `$HOME`, so the lock isolates the set/restore window.
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = tempfile::tempdir().unwrap();
    let _guard = HomeGuard::set(home.path());
    let projects = home.path().join(".claude").join("projects");

    // cc bot with cwd=/tmp (encodes to `-tmp`); cfuse_bin never spawned by
    // chat.inject (spec §5.1: inject does not drive an engine run), so the
    // mock cc script content is irrelevant here — we only need the cc engine
    // kind so the handler takes the `ClaudeJsonlSink` branch.
    let (url, state) = support::spawn_app_with_mock_and_state("mock_cc.sh", "cfuse-cc").await;
    // Pre-seed the engine session id so the sink path is taken.
    state
        .sessions
        .set_engine_session_id("worker-1", "s-1", "cc-sess-1")
        .await;

    let client = reqwest::Client::new();
    let body = json!({"type":"req","id":"inj-1","method":"chat.inject",
        "session_id":"s-1",
        "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
        "message":{"role":"user","content":[{"type":"text","text":"观察"}]},
        "from":{"kind":"bot","name":"张三"}});
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&body).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));

    // (a) The cc transcript file received one user entry tagged with the
    // inject's run_id (bridgeInjectId), living at
    // `<home>/.claude/projects/-tmp/cc-sess-1.jsonl` (cwd `/tmp`→`-tmp`).
    let transcript = projects.join("-tmp").join("cc-sess-1.jsonl");
    let content = std::fs::read_to_string(&transcript)
        .expect("transcript file was created on sink success");
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "exactly one user entry: {content}");
    let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(entry["type"], json!("user"));
    assert_eq!(entry["bridgeInjectId"], json!("inj-1"));
    assert_eq!(entry["sessionId"], json!("cc-sess-1"));
    assert_eq!(
        entry["message"]["content"][0]["text"],
        json!("[from:张三] 观察")
    );

    // (b) `pending_injects` stayed empty — a regression that always calls
    // `add_inject` after a successful sink would surface here (the next
    // chat.send would then both read the transcript AND prepend the prompt).
    let pending = state
        .sessions
        .take_pending_injects("worker-1", "s-1")
        .await;
    assert!(pending.is_empty(), "sink success must NOT also add_inject: {pending:?}");

    // Replay: same id empotently serves the prior {"ok":true} and does NOT
    // append a second transcript line (per-run_id idempotency).
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&body).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));
    let content2 = std::fs::read_to_string(&transcript).unwrap();
    let lines2: Vec<&str> = content2.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines2.len(), 1, "idempotent replay appended a second line: {content2}");
}

/// Task 14 — chat.abort terminal-state matrix:
/// - Active run present: 200 `{ok, aborted:true, aborted_run_ids:[run_id]}`; the
///   SSE stream for the aborted chat.send must emit a terminal `state=aborted`
///   frame (the 200 abort ACK does not wait for the engine to die — coordination
///   is via the run loop, which the test asserts by reading the SSE body).
/// - Repeating abort on the same now-terminal run: 410 `run_terminated` (stable;
///   the run→session reverse index on RunRegistry remembers the terminal run).
/// - Unknown session: 200 `{ok, aborted:false, aborted_run_ids:[]}`.
#[tokio::test]
async fn abort_active_run_emits_aborted_terminal() {
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let send = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-1","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"慢任务"}]}}));
    // 后台持有 SSE 响应体
    let sse = tokio::spawn(async move { send.send().await.unwrap().text().await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await; // 等 run 起跑

    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-1","method":"chat.abort",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["aborted"], serde_json::json!(true));
    assert_eq!(body["aborted_run_ids"], serde_json::json!(["run-1"]));

    let sse_text = tokio::time::timeout(std::time::Duration::from_secs(5), sse).await.unwrap().unwrap();
    assert!(sse_text.contains("\"state\":\"aborted\""));

    // 对同一 terminal run 重复 abort → 410 run_terminated（稳定）
    let again = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-2","method":"chat.abort",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    assert_eq!(again.status(), 410);
    assert_eq!(again.json::<serde_json::Value>().await.unwrap()["error"]["code"],
               serde_json::json!("run_terminated"));

    // 无任何记录的 session → aborted:false
    let none = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-3","method":"chat.abort",
            "session_id":"s-unknown",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    let none_body: serde_json::Value = none.json().await.unwrap();
    assert_eq!(none_body["aborted"], serde_json::json!(false));
    assert_eq!(none_body["aborted_run_ids"], serde_json::json!([]));
}
