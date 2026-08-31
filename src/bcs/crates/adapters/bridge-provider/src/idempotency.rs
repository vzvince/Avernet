use std::{collections::HashMap, sync::Mutex};

pub enum IdemDecision { Proceed, Replay(serde_json::Value), Conflict }

enum Entry { InProgress { fingerprint: String }, Completed { fingerprint: String, response: serde_json::Value } }

#[derive(Default)]
pub struct IdempotencyLedger { map: Mutex<HashMap<String, Entry>> }

impl IdempotencyLedger {
    pub fn new() -> Self { Self::default() }

    pub fn begin(&self, id: &str, fingerprint: &str) -> IdemDecision {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        match map.get(id) {
            Some(Entry::InProgress { fingerprint: f }) if f == fingerprint =>
                IdemDecision::Replay(serde_json::json!({"ok": true})),
            Some(Entry::Completed { fingerprint: f, response }) if f == fingerprint =>
                IdemDecision::Replay(response.clone()),
            Some(_) => IdemDecision::Conflict,
            None => {
                map.insert(id.to_string(), Entry::InProgress { fingerprint: fingerprint.to_string() });
                IdemDecision::Proceed
            }
        }
    }

    pub fn complete(&self, id: &str, response: serde_json::Value) {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(Entry::InProgress { fingerprint }) = map.get(id) {
            let fingerprint = fingerprint.clone();
            map.insert(id.to_string(), Entry::Completed { fingerprint, response });
        }
    }
}

pub fn fingerprint(parts: &[&str]) -> String {
    // 稳定拼接；调用方传入已选定的关键字段，避免引入哈希依赖
    parts.join("\u{1f}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_same_id_same_body_and_conflicts_different_body() {
        let ledger = IdempotencyLedger::new();
        assert!(matches!(ledger.begin("id-1", "fp-a"), IdemDecision::Proceed));
        ledger.complete("id-1", serde_json::json!({"ok": true}));
        match ledger.begin("id-1", "fp-a") {
            IdemDecision::Replay(v) => assert_eq!(v["ok"], serde_json::json!(true)),
            _ => panic!("expected replay"),
        }
        assert!(matches!(ledger.begin("id-1", "fp-b"), IdemDecision::Conflict));
    }

    #[test]
    fn in_progress_same_body_replays_ok_ack() {
        let ledger = IdempotencyLedger::new();
        assert!(matches!(ledger.begin("id-2", "fp-a"), IdemDecision::Proceed));
        match ledger.begin("id-2", "fp-a") {
            IdemDecision::Replay(v) => assert_eq!(v["ok"], serde_json::json!(true)),
            _ => panic!("expected replay"),
        }
    }
}
