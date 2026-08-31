pub mod cli;

use std::path::PathBuf;
use std::sync::Arc;

use bcs_protocol::stream::StreamEvent;

pub use crate::config::EngineKind;
use crate::config::BotConfig;

/// One BCS downstream turn request.
///
/// `run_id` is the BCS downstream body id; frames use it as `runId`.
/// `engine_session_id` carries the engine-native session id on follow-up turns.
/// `cfuse_bin` is the resolved engine binary path.
///
/// Note: an `interactions` field is added later by Task 12 together with the
/// `InteractionRegistry` it needs; this task defines only the fields below.
pub struct TurnRequest {
    pub run_id: String,
    pub prompt: String,
    pub engine_session_id: Option<String>,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub cfuse_bin: PathBuf,
    pub permission_mode: Option<String>,
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
#[derive(Debug)]
pub enum TurnError {
    Spawn(std::io::Error),
    EngineExited(String),
    Aborted,
    Protocol(String),
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

/// Placeholder engine returned by `build_engine` until Tasks 9/10 wire the
/// real `CfuseCc`/`CfuseCodex` drivers. Its `run_turn` immediately returns
/// `Err(TurnError::EngineExited("engine not wired"))` — production code must
/// not panic, so this never uses `unimplemented!`.
struct StubEngine {
    kind: EngineKind,
}

#[async_trait::async_trait]
impl Engine for StubEngine {
    fn kind(&self) -> EngineKind {
        self.kind
    }
    async fn run_turn(
        &self,
        _req: TurnRequest,
        _events: tokio::sync::mpsc::Sender<StreamEvent>,
        _abort: tokio_util::sync::CancellationToken,
    ) -> Result<TurnOutcome, TurnError> {
        Err(TurnError::EngineExited("engine not wired".into()))
    }
}

/// Build an [`Engine`] for `bot`. Until Tasks 9/10 land the real drivers, this
/// returns a [`StubEngine`] that records the configured engine kind and fails
/// every turn with `EngineExited("engine not wired")`.
pub fn build_engine(bot: &BotConfig) -> Arc<dyn Engine> {
    Arc::new(StubEngine { kind: bot.engine })
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
        };
        let outcome = engine.run_turn(req, tx, tokio_util::sync::CancellationToken::new()).await.unwrap();
        assert_eq!(outcome.engine_session_id.as_deref(), Some("e-1"));
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn build_engine_stub_errors_with_configured_kind() {
        let bot = BotConfig {
            provider_bot_ref: "cc-worker".into(),
            engine: EngineKind::CfuseCodex,
            model: None,
            cwd: "/tmp".into(),
            permission_mode: None,
            cfuse_bin: None,
        };
        let engine = build_engine(&bot);
        assert_eq!(engine.kind(), EngineKind::CfuseCodex);
        let (tx, _rx) = tokio::sync::mpsc::channel::<StreamEvent>(1);
        let req = TurnRequest {
            run_id: "r-1".into(), prompt: "hi".into(), engine_session_id: None,
            cwd: "/tmp".into(), model: None, cfuse_bin: "cfuse".into(), permission_mode: None,
        };
        let err = engine
            .run_turn(req, tx, tokio_util::sync::CancellationToken::new())
            .await
            .expect_err("stub must error");
        match err {
            TurnError::EngineExited(msg) => assert_eq!(msg, "engine not wired"),
            other => panic!("expected EngineExited, got {other:?}"),
        }
    }
}
