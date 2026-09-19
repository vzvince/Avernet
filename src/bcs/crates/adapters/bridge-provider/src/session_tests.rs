use super::*;

#[cfg(unix)]
#[tokio::test]
async fn database_alias_cannot_steal_an_active_inject_batch() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s", message("i", "saved"), "payload".into()).await.unwrap();
    store.claim_injects("worker-a", "s", "active").await.unwrap();
    let alias = fixture.dir.path().join("alias.sqlite3");
    std::os::unix::fs::symlink(&fixture.path, &alias).unwrap();
    assert!(matches!(SessionStore::open(&alias, "provider-a", &fixture.bots), Err(SessionError::Locked(_))),
        "a symlink alias must not recover another Bridge's active batch");
    store.complete_injects("worker-a", "s", "active").await.unwrap();
    assert!(store.claim_injects("worker-a", "s", "next").await.unwrap().is_empty());
}

struct Fixture {
    dir: tempfile::TempDir,
    path: std::path::PathBuf,
    bots: Vec<BotConfig>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge-state.sqlite3");
        let bots = ["worker-a", "worker-b"].into_iter().map(|name| BotConfig {
            provider_bot_ref: name.into(), engine: EngineKind::CfuseCc,
            model: None, cwd: dir.path().to_owned(), permission_mode: None, cfuse_bin: None,
        }).collect();
        Self { dir, path, bots }
    }

    fn open(&self, provider: &str) -> SessionStore {
        SessionStore::open(&self.path, provider, &self.bots).unwrap()
    }
}

fn message(id: &str, text: &str) -> InjectedMessage {
    InjectedMessage { run_id: id.into(), from_name: Some("张三".into()), text: text.into() }
}

fn ids(messages: &[InjectedMessage]) -> Vec<&str> {
    messages.iter().map(|message| message.run_id.as_str()).collect()
}

async fn query_only(store: &SessionStore, enabled: bool) {
    store.access(move |conn| {
        conn.pragma_update(None, "query_only", enabled)?;
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
async fn mapping_survives_app_state_recreation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, format!(r#"
provider_id = "bridge-1"
listen = "127.0.0.1:0"
bcs_to_provider_token = "test-token"
state_path = "session-state.sqlite3"
[[bot]]
provider_bot_ref = "worker"
engine = "cfuse-cc"
cwd = {:?}
"#, dir.path())).unwrap();
    let config = crate::config::ProviderConfig::load(&path).unwrap();
    let first = crate::AppState::new(config.clone()).unwrap();
    first.sessions.set_engine_session_id("worker", "s-1", "engine-1").await.unwrap();
    drop(first);
    let second = crate::AppState::new(config).unwrap();
    assert_eq!(second.sessions.mapping("worker", "s-1").await.unwrap().engine_session_id.as_deref(),
        Some("engine-1"), "Bridge restart lost the resume mapping");
}

#[tokio::test]
async fn mappings_and_injects_are_isolated_by_provider_bot_and_session() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    for (bot, session, engine) in [
        ("worker-a", "s-1", "engine-a"),
        ("worker-b", "s-1", "engine-b"),
        ("worker-a", "s-2", "engine-c"),
    ] {
        store.set_engine_session_id(bot, session, engine).await.unwrap();
    }
    store.enqueue_inject("worker-a", "s-1", message("shared-id", "provider a"), "payload-a".into()).await.unwrap();
    store.enqueue_inject("worker-b", "s-1", message("other-bot", "bot b"), "payload-b".into()).await.unwrap();
    store.enqueue_inject("worker-a", "s-2", message("other-session", "session 2"), "payload-c".into()).await.unwrap();
    drop(store);

    let other_provider = fixture.open("provider-b");
    assert!(other_provider.mapping("worker-a", "s-1").await.unwrap().engine_session_id.is_none());
    assert!(other_provider.claim_injects("worker-a", "s-1", "run-b").await.unwrap().is_empty());
    other_provider.set_engine_session_id("worker-a", "s-1", "engine-d").await.unwrap();
    other_provider.enqueue_inject("worker-a", "s-1", message("shared-id", "provider b"), "payload-d".into()).await.unwrap();
    let batch = other_provider.claim_injects("worker-a", "s-1", "run-b").await.unwrap();
    assert_eq!(ids(&batch), ["shared-id"]);
    assert_eq!(batch[0].text, "provider b");
    other_provider.complete_injects("worker-a", "s-1", "run-b").await.unwrap();
    drop(other_provider);

    let reopened = fixture.open("provider-a");
    for (bot, session, engine, inject) in [
        ("worker-a", "s-1", "engine-a", "shared-id"),
        ("worker-b", "s-1", "engine-b", "other-bot"),
        ("worker-a", "s-2", "engine-c", "other-session"),
    ] {
        assert_eq!(reopened.mapping(bot, session).await.unwrap().engine_session_id.as_deref(), Some(engine));
        assert_eq!(ids(&reopened.claim_injects(bot, session, "run-a").await.unwrap()), [inject]);
    }
}

#[tokio::test]
async fn active_run_exclusion_is_shared_in_process_and_cleared_on_reopen() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.try_start_run("worker-a", "s-1", "run-1").await.unwrap();
    let clone = store.clone();
    assert!(clone.try_start_run("worker-a", "s-1", "run-2").await.is_err());
    clone.try_start_run("worker-b", "s-1", "other-bot").await.unwrap();
    clone.try_start_run("worker-a", "s-2", "other-session").await.unwrap();
    store.finish_run("worker-a", "s-1", "wrong-owner").await;
    assert_eq!(clone.active_run("worker-a", "s-1").await.as_deref(), Some("run-1"));
    store.finish_run("worker-a", "s-1", "run-1").await;
    clone.try_start_run("worker-a", "s-1", "run-2").await.unwrap();
    drop(clone);
    drop(store);
    let reopened = fixture.open("provider-a");
    assert!(reopened.active_run("worker-a", "s-1").await.is_none());
    reopened.try_start_run("worker-a", "s-1", "run-3").await.unwrap();
}

#[tokio::test]
async fn claim_is_fifo_and_completion_leaves_later_injects_for_the_next_run() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s-1", message("first", "first message"), "one".into()).await.unwrap();
    store.enqueue_inject("worker-a", "s-1", message("second", "第二条消息"), "two".into()).await.unwrap();
    let batch = store.claim_injects("worker-a", "s-1", "run-1").await.unwrap();
    assert_eq!(ids(&batch), ["first", "second"]);
    assert_eq!(batch[1].text, "第二条消息");
    assert_eq!(batch[1].from_name.as_deref(), Some("张三"));
    store.enqueue_inject("worker-a", "s-1", message("later", "next turn"), "three".into()).await.unwrap();
    store.complete_injects("worker-a", "s-1", "run-1").await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "run-2").await.unwrap()), ["later"]);
    store.complete_injects("worker-a", "s-1", "run-2").await.unwrap();
    assert!(store.claim_injects("worker-a", "s-1", "run-3").await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_and_interrupted_attempts_retry_the_original_fifo_batch() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s-1", message("first", "retry me"), "one".into()).await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "failed").await.unwrap()), ["first"]);
    store.release_injects("worker-a", "s-1", "failed").await.unwrap();
    store.enqueue_inject("worker-a", "s-1", message("second", "later message"), "two".into()).await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "interrupted").await.unwrap()), ["first", "second"]);
    // Reopen without releasing or completing the claimed messages, as after a crash.
    drop(store);
    let reopened = fixture.open("provider-a");
    assert_eq!(ids(&reopened.claim_injects("worker-a", "s-1", "recovered").await.unwrap()), ["first", "second"]);
    reopened.complete_injects("worker-a", "s-1", "recovered").await.unwrap();
    assert!(reopened.claim_injects("worker-a", "s-1", "last").await.unwrap().is_empty());
}

#[tokio::test]
async fn delivered_receipts_deduplicate_and_reject_conflicts_after_reopen() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s-1", message("receipt", "original"), "same-payload".into()).await.unwrap();
    store.enqueue_inject("worker-a", "s-1", message("receipt", "original"), "same-payload".into()).await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "run-1").await.unwrap()), ["receipt"]);
    store.complete_injects("worker-a", "s-1", "run-1").await.unwrap();
    drop(store);
    let reopened = fixture.open("provider-a");
    for (bot, session, text, fingerprint) in [
        ("worker-a", "s-1", "changed", "different-payload"),
        ("worker-b", "s-1", "original", "same-payload"),
        ("worker-a", "s-2", "original", "same-payload"),
    ] {
        assert!(matches!(reopened.enqueue_inject(bot, session, message("receipt", text), fingerprint.into()).await,
            Err(SessionError::Conflict)));
    }
    reopened.enqueue_inject("worker-a", "s-1", message("receipt", "original"), "same-payload".into()).await.unwrap();
    assert!(reopened.claim_injects("worker-a", "s-1", "run-2").await.unwrap().is_empty());
    assert!(reopened.claim_injects("worker-b", "s-1", "run-3").await.unwrap().is_empty());
    assert!(reopened.claim_injects("worker-a", "s-2", "run-4").await.unwrap().is_empty());
}

#[tokio::test]
async fn changed_engine_or_cwd_rejects_resume_without_overwriting_the_binding() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.set_engine_session_id("worker-a", "s-1", "original-engine-id").await.unwrap();
    drop(store);
    let other_cwd = fixture.dir.path().join("other-cwd");
    std::fs::create_dir(&other_cwd).unwrap();
    let mut changed_cwd = fixture.bots.clone();
    changed_cwd[0].cwd = other_cwd;
    let mut changed_engine = fixture.bots.clone();
    changed_engine[0].engine = EngineKind::CfuseCodex;
    for bots in [changed_cwd, changed_engine] {
        let changed = SessionStore::open(&fixture.path, "provider-a", &bots).unwrap();
        assert!(matches!(changed.mapping("worker-a", "s-1").await, Err(SessionError::BindingChanged)));
        assert!(matches!(changed.set_engine_session_id("worker-a", "s-1", "replacement").await,
            Err(SessionError::BindingChanged)));
        drop(changed);
    }
    let original = fixture.open("provider-a");
    assert_eq!(original.mapping("worker-a", "s-1").await.unwrap().engine_session_id.as_deref(), Some("original-engine-id"));
}

#[tokio::test]
async fn engine_session_id_cannot_change_or_become_invalid() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    for invalid in ["", "../outside", "--option", "contains space", "a/b", "a\\b"] {
        assert!(matches!(store.set_engine_session_id("worker-a", "s-1", invalid).await,
            Err(SessionError::InvalidSessionId)));
    }
    assert!(store.mapping("worker-a", "s-1").await.unwrap().engine_session_id.is_none());
    store.set_engine_session_id("worker-a", "s-1", "engine-original").await.unwrap();
    store.set_engine_session_id("worker-a", "s-1", "engine-original").await.unwrap();
    assert!(matches!(store.set_engine_session_id("worker-a", "s-1", "engine-replacement").await,
        Err(SessionError::SessionChanged)));
    assert_eq!(store.mapping("worker-a", "s-1").await.unwrap().engine_session_id.as_deref(), Some("engine-original"));
}

#[tokio::test]
async fn invalid_persisted_engine_id_is_rejected_on_resume() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.set_engine_session_id("worker-a", "s-1", "engine-original").await.unwrap();
    store.access(|conn| {
        conn.execute("UPDATE sessions SET engine_session_id = '../unsafe'", [])?;
        Ok(())
    }).await.unwrap();
    drop(store);
    let reopened = fixture.open("provider-a");
    assert!(matches!(reopened.mapping("worker-a", "s-1").await, Err(SessionError::InvalidSessionId)));
}

#[test]
fn database_allows_only_one_owner_until_the_last_store_clone_is_dropped() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    let clone = store.clone();
    assert!(matches!(SessionStore::open(&fixture.path, "provider-b", &fixture.bots), Err(SessionError::Locked(_))));
    drop(store);
    assert!(matches!(SessionStore::open(&fixture.path, "provider-a", &fixture.bots), Err(SessionError::Locked(_))));
    drop(clone);
    let reopened = fixture.open("provider-a");
    drop(reopened);
}

#[test]
fn corrupt_database_and_unsupported_schema_fail_instead_of_starting_empty() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.path, b"this is not a SQLite database").unwrap();
    assert!(matches!(SessionStore::open(&fixture.path, "provider-a", &fixture.bots), Err(SessionError::Database(_))));
    assert_eq!(std::fs::read(&fixture.path).unwrap(), b"this is not a SQLite database");

    let schema_path = fixture.dir.path().join("future.sqlite3");
    let conn = rusqlite::Connection::open(&schema_path).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    assert!(matches!(SessionStore::open(&schema_path, "provider-a", &fixture.bots), Err(SessionError::Schema(999))));
}

#[tokio::test]
async fn failed_enqueue_and_claim_propagate_database_errors_without_losing_messages() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s-1", message("saved", "already acknowledged"), "saved-payload".into()).await.unwrap();
    query_only(&store, true).await;
    assert!(matches!(store.enqueue_inject("worker-a", "s-1", message("rejected", "not committed"), "new-payload".into()).await,
        Err(SessionError::Database(_))));
    assert!(matches!(store.claim_injects("worker-a", "s-1", "read-only").await, Err(SessionError::Database(_))));
    assert!(matches!(store.set_engine_session_id("worker-a", "s-1", "not-saved").await, Err(SessionError::Database(_))));
    query_only(&store, false).await;
    assert!(store.mapping("worker-a", "s-1").await.unwrap().engine_session_id.is_none());
    let batch = store.claim_injects("worker-a", "s-1", "writable").await.unwrap();
    assert_eq!(ids(&batch), ["saved"]);
    assert_eq!(batch[0].text, "already acknowledged");
    store.complete_injects("worker-a", "s-1", "writable").await.unwrap();
    store.enqueue_inject("worker-a", "s-1", message("rejected", "not committed"), "new-payload".into()).await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "retry-enqueue").await.unwrap()), ["rejected"]);
}

#[tokio::test]
async fn failed_completion_or_release_keeps_the_claimed_batch_retryable() {
    let fixture = Fixture::new();
    let store = fixture.open("provider-a");
    store.enqueue_inject("worker-a", "s-1", message("saved", "keep until committed"), "payload".into()).await.unwrap();
    assert_eq!(ids(&store.claim_injects("worker-a", "s-1", "failed-write").await.unwrap()), ["saved"]);
    query_only(&store, true).await;
    assert!(matches!(store.complete_injects("worker-a", "s-1", "failed-write").await, Err(SessionError::Database(_))));
    assert!(matches!(store.release_injects("worker-a", "s-1", "failed-write").await, Err(SessionError::Database(_))));
    query_only(&store, false).await;
    let batch = store.claim_injects("worker-a", "s-1", "retry").await.unwrap();
    assert_eq!(ids(&batch), ["saved"]);
    assert_eq!(batch[0].text, "keep until committed");
    store.complete_injects("worker-a", "s-1", "failed-write").await.unwrap();
    store.release_injects("worker-a", "s-1", "failed-write").await.unwrap();
    store.complete_injects("worker-a", "s-1", "retry").await.unwrap();
    drop(store);
    let reopened = fixture.open("provider-a");
    assert!(reopened.claim_injects("worker-a", "s-1", "after-restart").await.unwrap().is_empty());
}
