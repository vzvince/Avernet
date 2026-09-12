use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct InjectedMessage {
    pub run_id: String,
    pub from_name: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionMapping {
    pub engine_session_id: Option<String>,
    pub pending_injects: Vec<InjectedMessage>,
    pub active_run: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("session already has an active run")]
pub struct SessionBusy;

type Key = (String, String); // (provider_bot_ref, bcs_session_id)

#[derive(Clone, Default)]
pub struct SessionStore {
    map: Arc<RwLock<HashMap<Key, SessionMapping>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn mapping(&self, bot: &str, s: &str) -> SessionMapping {
        self.map
            .read()
            .await
            .get(&(bot.into(), s.into()))
            .cloned()
            .unwrap_or_default()
    }

    pub async fn set_engine_session_id(&self, bot: &str, s: &str, engine_id: &str) {
        self.map
            .write()
            .await
            .entry((bot.into(), s.into()))
            .or_default()
            .engine_session_id = Some(engine_id.into());
    }

    pub async fn add_inject(&self, bot: &str, s: &str, msg: InjectedMessage) {
        self.map
            .write()
            .await
            .entry((bot.into(), s.into()))
            .or_default()
            .pending_injects.push(msg);
    }

    pub async fn take_pending_injects(&self, bot: &str, s: &str) -> Vec<InjectedMessage> {
        let mut map = self.map.write().await;
        match map.get_mut(&(bot.into(), s.into())) {
            Some(m) => std::mem::take(&mut m.pending_injects),
            None => Vec::new(),
        }
    }

    pub async fn try_start_run(&self, bot: &str, s: &str, run_id: &str) -> Result<(), SessionBusy> {
        let mut map = self.map.write().await;
        let m = map.entry((bot.into(), s.into())).or_default();
        if m.active_run.is_some() {
            return Err(SessionBusy);
        }
        m.active_run = Some(run_id.into());
        Ok(())
    }

    pub async fn finish_run(&self, bot: &str, s: &str, run_id: &str) {
        let mut map = self.map.write().await;
        if let Some(m) = map.get_mut(&(bot.into(), s.into())) {
            if m.active_run.as_deref() == Some(run_id) {
                m.active_run = None;
            }
        }
    }

    pub async fn active_run(&self, bot: &str, s: &str) -> Option<String> {
        self.map
            .read()
            .await
            .get(&(bot.into(), s.into()))
            .and_then(|m| m.active_run.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dual_id_mapping_and_run_exclusion() {
        let store = SessionStore::new();
        let m = store.mapping("bot-a", "s-1").await;
        assert!(m.engine_session_id.is_none());

        store.set_engine_session_id("bot-a", "s-1", "engine-sess-9").await;
        assert_eq!(store.mapping("bot-a", "s-1").await.engine_session_id.as_deref(),
                   Some("engine-sess-9"));
        // 另一个 bcs session 不受影响
        assert!(store.mapping("bot-a", "s-2").await.engine_session_id.is_none());

        store.try_start_run("bot-a", "s-1", "run-1").await.unwrap();
        assert!(store.try_start_run("bot-a", "s-1", "run-2").await.is_err());
        store.finish_run("bot-a", "s-1", "run-1").await;
        store.try_start_run("bot-a", "s-1", "run-2").await.unwrap();
    }

    #[tokio::test]
    async fn pending_injects_fifo_drain() {
        let store = SessionStore::new();
        store.add_inject("b", "s", InjectedMessage{ run_id: "i1".into(), from_name: None, text: "m1".into() }).await;
        store.add_inject("b", "s", InjectedMessage{ run_id: "i2".into(), from_name: Some("张三".into()), text: "m2".into() }).await;
        let drained = store.take_pending_injects("b", "s").await;
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "m1");
        assert!(store.take_pending_injects("b", "s").await.is_empty());
    }
}
