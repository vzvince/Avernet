use std::{net::SocketAddr, sync::Arc};
use bridge_provider::{config::ProviderConfig, webhook};

pub async fn spawn_app(toml_text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, toml_text).unwrap();
    let config = ProviderConfig::load(&path).unwrap();
    // tempdir 不能 drop：泄漏到测试生命周期结束即可（测试进程退出清理）
    std::mem::forget(dir);
    let mut cfg = config;
    cfg.listen = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let app = webhook::router(Arc::new(bridge_provider::AppState::new(cfg)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// 用指定 mock 脚本作为 cfuse binary 起服务（engine 由参数选择 cc/codex 语义）。
///
/// 单 bot `worker-1`，cfuse_bin 指向 `tests/fixtures/{script}`；两个端到端
/// 测试都经此构造，避免真实 LLM 调用。
pub async fn spawn_app_with_mock(script: &str, engine: &str) -> String {
    let bin = format!("{}/tests/fixtures/{script}", env!("CARGO_MANIFEST_DIR"));
    spawn_app(&format!(
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
