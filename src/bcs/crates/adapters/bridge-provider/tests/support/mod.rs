use std::{net::SocketAddr, sync::Arc};
use bridge_provider::{config::ProviderConfig, webhook, AppState};

pub async fn spawn_app(toml_text: &str) -> String {
    spawn_app_with_state(toml_text).await.0
}

/// 与 [`spawn_app`] 同样起服务，但额外返回共享的 [`AppState`] 句柄，供测试
/// 直接编排会话状态（例如预置 `engine_session_id`、排空 `pending_injects`）。
pub async fn spawn_app_with_state(toml_text: &str) -> (String, Arc<AppState>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, toml_text).unwrap();
    let config = ProviderConfig::load(&path).unwrap();
    // tempdir 不能 drop：泄漏到测试生命周期结束即可（测试进程退出清理）
    std::mem::forget(dir);
    let mut cfg = config;
    cfg.listen = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let state = Arc::new(AppState::new(cfg));
    let app = webhook::router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), state)
}

/// 用指定 mock 脚本作为 cfuse binary 起服务（engine 由参数选择 cc/codex 语义）。
///
/// 单 bot `worker-1`，cfuse_bin 指向 `tests/fixtures/{script}`；两个端到端
/// 测试都经此构造，避免真实 LLM 调用。
pub async fn spawn_app_with_mock(script: &str, engine: &str) -> String {
    spawn_app_with_mock_and_state(script, engine).await.0
}

/// [`spawn_app_with_mock`] 的 state-暴露版本：额外返回共享 [`AppState`]。
pub async fn spawn_app_with_mock_and_state(script: &str, engine: &str) -> (String, Arc<AppState>) {
    let bin = format!("{}/tests/fixtures/{script}", env!("CARGO_MANIFEST_DIR"));
    spawn_app_with_state(&format!(
        r#"
provider_id = "bridge-1"
listen = "127.0.0.1:0"
bcs_to_provider_token = "tok-b2p"
[[bot]]
provider_bot_ref = "worker-1"
engine = "{engine}"
cwd = "/tmp"
cfuse_bin = "{bin}"
"#
    ))
    .await
}

/// 从 SSE 文本抽取所有 data 帧里的 seq 序列（按出现顺序）。非 data 行/非
/// JSON/无 seq 字段均跳过；用于断言 seq 单调递增。
pub fn extract_seqs(sse_text: &str) -> Vec<u64> {
    sse_text
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter_map(|v| v["seq"].as_u64())
        .collect()
}

/// 从 SSE 文本抽取首个 interaction 帧的 `interactionId`。用于端到端
/// interaction 回环测试：BCS 侧拿到 iid 再调 `interaction.resolve`。
pub fn extract_first_interaction_id(sse_text: &str) -> String {
    sse_text
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .find(|v| v["interactionId"].is_string())
        .and_then(|v| v["interactionId"].as_str().map(str::to_string))
        .expect("interaction requested frame present")
}
