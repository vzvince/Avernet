use axum::http::StatusCode;
use serde_json::json;

mod support; // tests/support/mod.rs：spawn_app(config_toml: &str) -> String(base_url)

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
