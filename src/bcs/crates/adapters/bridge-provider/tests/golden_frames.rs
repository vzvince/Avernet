use bridge_provider::sse::*;
use bcs_protocol::stream::{parse_stream_event, ChatState, StreamEvent, ToolPhase};
use serde_json::json;

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

/// 从编码帧中抽出 event 名与 data JSON（测试辅助）
fn split_frame(frame: &str) -> (String, serde_json::Value) {
    let mut event = String::new();
    let mut data = String::new();
    for line in frame.lines() {
        if let Some(v) = line.strip_prefix("event: ") {
            event = v.to_string();
        }
        if let Some(v) = line.strip_prefix("data: ") {
            data = v.to_string();
        }
    }
    (event, serde_json::from_str(&data).unwrap())
}

#[test]
fn chat_delta_roundtrips_through_bcs_parser() {
    let frame = event_to_frame(&chat_delta("r-1", "正在分析"), 1, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    assert_eq!(event, "chat");
    match parse_stream_event(&event, data) {
        StreamEvent::Chat(c) => {
            assert_eq!(c.state, ChatState::Delta);
            assert_eq!(c.delta_text.as_deref(), Some("正在分析"));
            assert_eq!(c.seq, Some(1));
        }
        other => panic!("expected chat, got {other:?}"),
    }
}

#[test]
fn chat_final_is_full_snapshot_terminal() {
    let frame = event_to_frame(&chat_final("r-1", "最终答案".to_string()), 5, 200, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    match parse_stream_event(&event, data.clone()) {
        StreamEvent::Chat(c) => {
            assert_eq!(c.state, ChatState::Final);
            assert_eq!(data["message"]["content"][0]["text"], json!("最终答案"));
        }
        other => panic!("expected final, got {other:?}"),
    }
}

#[test]
fn tool_result_roundtrips() {
    let ev = agent_tool(
        "r-1",
        bcs_protocol::stream::ToolData {
            phase: ToolPhase::Result,
            name: Some("exec".into()),
            tool_call_id: Some("tc-1".into()),
            is_error: Some(false),
            exit_code: Some(0),
            duration_ms: Some(120),
            cwd: None,
            args: None,
            result: Some(json!({"content":[{"type":"text","text":"ok"}]})),
            partial_result: None,
        },
    );
    let frame = event_to_frame(&ev, 4, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    match parse_stream_event(&event, data) {
        StreamEvent::Agent(a) => match a.data {
            bcs_protocol::stream::AgentData::Tool(t) => {
                assert_eq!(t.phase, ToolPhase::Result);
                assert_eq!(t.tool_call_id.as_deref(), Some("tc-1"));
            }
            other => panic!("expected tool, got {other:?}"),
        },
        other => panic!("expected agent, got {other:?}"),
    }
}

#[test]
fn interaction_requested_exec_roundtrips() {
    let ev = interaction_event(
        "r-1",
        bcs_protocol::stream::InteractionPhase::Requested,
        bcs_protocol::stream::InteractionKind::Exec,
        "int-1",
        json!({"title":"Run command?","command":"npm run deploy",
               "options":[{"decision":"allow_once","label":"Allow once"},
                          {"decision":"deny","label":"Deny"}]}),
    );
    let frame = event_to_frame(&ev, 7, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    assert_eq!(event, "interaction");
    match parse_stream_event(&event, data.clone()) {
        StreamEvent::Interaction(i) => {
            assert_eq!(i.interaction_id, "int-1");
            assert_eq!(i.kind, bcs_protocol::stream::InteractionKind::Exec);
            assert_eq!(data["options"][0]["decision"], json!("allow_once"));
        }
        other => panic!("expected interaction, got {other:?}"),
    }
}
