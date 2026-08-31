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
