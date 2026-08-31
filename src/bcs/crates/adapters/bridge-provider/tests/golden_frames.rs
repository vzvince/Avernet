use bridge_provider::sse::{encode_frame, FrameError, HEARTBEAT, MAX_FRAME_BYTES};

#[test]
fn encodes_chat_delta_golden() {
    let frame = encode_frame(
        "chat",
        Some(605),
        r#"{"state":"delta","deltaText":"查询。","runId":"r-1","seq":605,"ts":1786276303908}"#,
    )
    .unwrap();
    let expected = "event: chat\nid: 605\ndata: {\"state\":\"delta\",\"deltaText\":\"查询。\",\"runId\":\"r-1\",\"seq\":605,\"ts\":1786276303908}\n\n";
    assert_eq!(frame, expected);
}

#[test]
fn encodes_frame_without_id() {
    let frame = encode_frame("ping", None, r#"{"ts":1}"#).unwrap();
    assert_eq!(frame, "event: ping\ndata: {\"ts\":1}\n\n");
}

#[test]
fn rejects_frame_over_8mib() {
    let big = "x".repeat(MAX_FRAME_BYTES);
    let data_json = format!(r#"{{"deltaText":"{}"}}"#, big);
    let err = encode_frame("chat", None, &data_json).unwrap_err();
    assert!(matches!(err, FrameError::FrameTooLarge(_)));
}

#[test]
fn rejects_multiline_data() {
    let err = encode_frame("chat", None, "{\"ts\":1}\n{\"ts\":2}").unwrap_err();
    assert!(matches!(err, FrameError::MultilineData));
}

#[test]
fn heartbeat_is_sse_comment() {
    assert_eq!(HEARTBEAT, ": heartbeat\n\n");
}
