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
