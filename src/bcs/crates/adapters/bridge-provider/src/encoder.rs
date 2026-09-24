//! Pure protocol conversion. Adapters own connections; runtime owns execution.
use bcs_protocol::{BcsFrame, RequestFrame, ResponseFrame, EventFrame, ErrorShape, MessageContent, ChatAbortParams};
use bcs_protocol::stream::{AgentData, ChatState, StreamEvent};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error::BridgeError, run::RunEvent, runtime::{DownstreamRequest, ToBot}, sse::{self, FrameError}};

pub trait Encoder {
    type Input;
    type Output;
    fn decode(&self, input: Self::Input) -> Result<DownstreamRequest, BridgeError>;
    fn encode(&self, event: &RunEvent) -> Result<Option<Self::Output>, FrameError>;
}

pub struct GatewayEncoder;
impl Encoder for GatewayEncoder {
    type Input = Value;
    type Output = String;
    fn decode(&self, input: Value) -> Result<DownstreamRequest, BridgeError> {
        serde_json::from_value(input).map_err(|_| BridgeError::invalid_request("invalid gateway request"))
    }
    fn encode(&self, event: &RunEvent) -> Result<Option<String>, FrameError> {
        if matches!(event.event, StreamEvent::Ping { .. }) {
            return Ok(Some(sse::HEARTBEAT.into()));
        }
        sse::event_to_frame(&event.event, event.seq, event.ts, &event.run_id).map(Some)
    }
}

pub struct WebSocketV2Encoder {
    provider_id: String,
    bot_ref: String,
    group_id: String,
}

#[derive(Deserialize)]
struct ChatInput {
    session_key: String,
    bcs_group_id: String,
    #[serde(default)]
    bcs_session_id: Option<String>,
    message: MessageContent,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    session_context: Value,
    #[serde(default)]
    from: Option<Value>,
}

/// V2 sends the full session ID as bcs_group_id for non-default sessions.
/// Default group deliveries must share the gateway's canonical default session.
pub fn canonical_session(id: &str) -> String {
    if !id.contains(':') && (id.starts_with("bcs_grp_") || id.starts_with("grp-")) {
        format!("{id}:00000000")
    } else {
        id.to_string()
    }
}

impl WebSocketV2Encoder {
    pub fn new(provider_id: &str, bot_ref: &str, group_id: &str) -> Self {
        Self { provider_id:provider_id.into(), bot_ref:bot_ref.into(), group_id:group_id.into() }
    }

    pub fn reply(id: &str, result: Result<Value, BridgeError>) -> BcsFrame {
        BcsFrame::Response(match result {
            Ok(body) => ResponseFrame::ok(id, body),
            Err(error) => ResponseFrame::err(id, ErrorShape {
                code:error.code.into(), message:error.message, retryable:error.retryable,
                details:None, retry_after_ms:None,
            }),
        })
    }
}

impl Encoder for WebSocketV2Encoder {
    type Input = RequestFrame;
    type Output = BcsFrame;

    fn decode(&self, input: RequestFrame) -> Result<DownstreamRequest, BridgeError> {
        if input.id.trim().is_empty() {
            return Err(BridgeError::invalid_request("request id required"));
        }
        let mut req = DownstreamRequest {
            id:input.id, method:input.method,
            to_bot:ToBot {provider_id:self.provider_id.clone(),provider_bot_ref:self.bot_ref.clone()},
            session_id:None, message:None, from:None, timeout_ms:None, params:input.params,
        };
        match req.method.as_str() {
            "bot.ping" => {}
            "chat.send" | "chat.inject" => {
                let params: ChatInput = serde_json::from_value(req.params.clone().unwrap_or(Value::Null))
                    .map_err(|_| BridgeError::invalid_request("invalid V2 chat parameters"))?;
                if params.session_key.trim().is_empty() || params.bcs_group_id.trim().is_empty() {
                    return Err(BridgeError::invalid_request("session_key and bcs_group_id required"));
                }
                let session = params.bcs_session_id.as_deref().unwrap_or(&params.bcs_group_id);
                if session.trim().is_empty() || session.split(':').next() != params.bcs_group_id.split(':').next()
                    || (params.bcs_group_id.contains(':') && params.bcs_group_id != session) {
                    return Err(BridgeError::invalid_request("session does not match bcs_group_id"));
                }
                req.session_id = Some(canonical_session(session));
                let message = serde_json::to_value(params.message)
                    .map_err(|_| BridgeError::invalid_request("invalid message"))?;
                if crate::run::extract_message_text(Some(&message)).trim().is_empty() {
                    return Err(BridgeError::invalid_request("a non-empty text message is required"));
                }
                req.message = Some(message);
                if req.method == "chat.send" {
                    if let Some(key) = params.idempotency_key {
                        if key.trim().is_empty() { return Err(BridgeError::invalid_request("empty idempotency_key")); }
                        req.id = key;
                    }
                }
                req.timeout_ms = params.timeout_ms;
                req.from = params.from;
                if let Some(name) = params.session_context.get("from").and_then(Value::as_str) {
                    req.from = Some(json!({"name":name,"actor_id":params.session_context.get("from_bot_id")}));
                }
            }
            "chat.abort" => {
                let params: ChatAbortParams = serde_json::from_value(req.params.clone().unwrap_or(Value::Null))
                    .map_err(|_| BridgeError::invalid_request("invalid V2 abort parameters"))?;
                if params.session_key.trim().is_empty() || params.run_id.as_deref().is_some_and(|s| s.trim().is_empty()) {
                    return Err(BridgeError::invalid_request("empty abort target"));
                }
                req.session_id = Some(canonical_session(&params.session_key));
            }
            method => return Err(BridgeError::unsupported_method(method)),
        }
        Ok(req)
    }

    fn encode(&self, event: &RunEvent) -> Result<Option<BcsFrame>, FrameError> {
        let (name, payload) = match &event.event {
            StreamEvent::Chat(chat) => {
                let mut payload = json!({"run_id":event.run_id,"bcs_group_id":self.group_id,
                    "state":match chat.state {ChatState::Delta=>"delta",ChatState::Final=>"final",ChatState::Error=>"error",ChatState::Aborted=>"aborted"}});
                let object = payload.as_object_mut().ok_or(FrameError::Unsupported)?;
                for (key, value) in [("delta_text",&chat.delta_text),("stop_reason",&chat.stop_reason),
                    ("errorMessage",&chat.error_message),("errorKind",&chat.error_kind),("errorCode",&chat.error_code)] {
                    if let Some(value) = value { object.insert(key.into(),json!(value)); }
                }
                if let Some(message) = &chat.message { object.insert("message".into(),message.clone()); }
                ("chat.event",payload)
            }
            StreamEvent::Agent(agent) => {
                let (stream, data) = match &agent.data {
                    AgentData::Tool(data) => ("tool",serde_json::to_value(data)?),
                    AgentData::Thinking(data) => ("thinking",serde_json::to_value(data)?),
                    AgentData::Lifecycle(data) => ("lifecycle",serde_json::to_value(data)?),
                    _ => return Err(FrameError::Unsupported),
                };
                ("agent", json!({"run_id":event.run_id,"bcs_group_id":self.group_id,
                    "stream":stream,"ts":event.ts,"data":data}))
            }
            StreamEvent::Ping { .. } => return Ok(None),
            _ => return Err(FrameError::Unsupported),
        };
        let frame = BcsFrame::Event(EventFrame::new(name,Some(payload),Some(event.seq)));
        let size = serde_json::to_vec(&frame)?.len();
        if size > sse::MAX_FRAME_BYTES { return Err(FrameError::FrameTooLarge(size)); }
        Ok(Some(frame))
    }
}
