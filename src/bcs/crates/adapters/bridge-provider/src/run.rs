//! RunRegistry + run loop: drives one downstream turn end-to-end and exposes a
//! self-managed SSE frame stream to the webhook handler.
//!
//! Per spec §6.2/§6.4: the run loop selects over an engine-event channel, a 20s
//! heartbeat, and a deadline timer; each engine event is stamped with a monotonic
//! `seq`, encoded to a Provider 2.0 frame via [`crate::sse::event_to_frame`], then
//! appended to the run buffer and broadcast. A BCS disconnect is detected when a
//! broadcast send returns `Err` (no live subscribers); the engine is then aborted
//! and the run closed (spec amendment: kill on write failure, no grace window).
//!
//! Frames are self-managed `String`s (already-formatted SSE frames) — the single
//! testable path; the handler wraps them with [`axum::body::Body::from_stream`].
//! Heartbeats push the raw [`crate::sse::HEARTBEAT`] comment frame and carry no
//! `seq` (excluded from the monotonic sequence).
//!
//! Re-attach semantics: the same id with the same body, while active, replays the
//! buffered frames then follows the broadcast; the same id already terminal
//! replays the buffered terminal frames as a fresh one-shot stream; the same id
//! with a different body is a 409 conflict (see [`RunRegistry::begin`]).

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::HeaderValue;
use axum::response::Response;
use bcs_protocol::now_ms;
use bcs_protocol::stream::{ChatState, StreamEvent};
use futures::stream::{StreamExt, Stream};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::engine::{build_engine, TurnError, TurnOutcome, TurnRequest};
use crate::engine::trace::TraceContext;
use crate::sse::{self, event_to_frame, FrameError, HEARTBEAT};
use crate::webhook::{AppState, DownstreamRequest};
use crate::config::BotConfig;

/// Grace TTL for a terminal run's buffered frames before lazy sweep removes the
/// entry (lets a late re-send replay the terminal state).
const TERMINAL_GRACE: Duration = Duration::from_secs(300);

/// Forward-loop poll interval: after the driver marks a run terminal, the
/// forwarder drains remaining broadcast messages and exits within this window.
/// Kept tiny so end-of-run latency is negligible; robust against lost wake-ups
/// because the drain is by `try_recv`, not by a one-shot Notify.
const TERMINAL_POLL: Duration = Duration::from_millis(25);

/// Result of attempting to push one frame into the run's buffer+broadcast.
enum PushOutcome {
    /// Frame accepted; loop continues.
    Ok,
    /// Broadcast had no subscribers: BCS disconnected → abort + close.
    Disconnect,
    /// Run must terminate now (oversize frame caught → terminal error emitted,
    /// or encoder rejected the event).
    Terminate,
}

/// One active or terminal run's shared state: abort token, broadcast sender,
/// replay buffer, terminal flag, and the idempotency fingerprint.
///
/// `buffer` is a `std::sync::Mutex` (not `tokio::RwLock`) so that a push and a
/// forwarder's `(subscribe, snapshot)` can be made mutually atomic without
/// holding the lock across an `.await` — neither path awaits while holding it.
/// This is what keeps the replay-buffer-then-follow-broadcast forward path free
/// of duplicate or lost frames.
///
/// `abort_requested` distinguishes an explicit abort (chat.abort or graceful
/// shutdown) from a passive BCS disconnect. The run loop emits a terminal
/// `chat_aborted` frame only when it is set, so a disconnect closes the stream
/// silently while chat.abort surfaces a final `state=aborted` frame to the BCS
/// SSE consumer (spec §5.3).
///
/// All fields are `Arc`/clone-cheap so a [`RunHandle`] is cheaply cloneable for
/// the driver task and each re-attach forwarder.
#[derive(Clone)]
pub struct RunHandle {
    pub abort: CancellationToken,
    pub tx: broadcast::Sender<String>,
    pub buffer: Arc<Mutex<Vec<String>>>,
    pub terminal: Arc<AtomicBool>,
    abort_requested: Arc<AtomicBool>,
    /// `stopReason` to surface in the terminal `chat_aborted` SSE frame. Set by
    /// the abort requester via [`Self::request_abort`]; stays at the default
    /// `"user_cancelled"` until then. `std::sync::Mutex` (short critical section,
    /// never held across an `.await`) so a poison is recoverable.
    abort_reason: Arc<Mutex<String>>,
    fp: Arc<String>,
}

impl RunHandle {
    /// Idempotency fingerprint match (same id + same body).
    pub fn matches(&self, fp: &str) -> bool {
        self.fp.as_ref() == fp
    }

    /// Whether the run has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.terminal.load(Ordering::SeqCst)
    }

    /// True iff an explicit abort has been requested via [`Self::request_abort`]
    /// (chat.abort or graceful shutdown). The run loop emits a terminal
    /// `chat_aborted` frame only when this is set; a passive BCS disconnect
    /// (broadcast send returns no subscribers) does not set it, so its run
    /// closes silently — the stream just ends.
    pub fn is_abort_requested(&self) -> bool {
        self.abort_requested.load(Ordering::SeqCst)
    }

    /// The `stopReason` to surface in the terminal `chat_aborted` SSE frame —
    /// whatever the most recent [`Self::request_abort`] caller set, defaulting
    /// to `"user_cancelled"` until then. Mutex poison is recovered (consistent
    /// with the rest of this crate) so a panicking holder never wedges the run.
    pub fn abort_stop_reason(&self) -> String {
        self.abort_reason.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Mark this run explicitly aborted: set the requested flag (so the run
    /// loop emits a `chat_aborted` terminal frame when the engine returns
    /// `TurnError::Aborted`), record `reason` as the SSE frame's `stopReason`,
    /// and cancel the token (the driver's `select!` arm fires, killing the
    /// engine). Idempotent — safe to call repeatedly (a chat.abort racing a
    /// graceful shutdown, or a duplicate abort, all collapse to one abort).
    pub fn request_abort(&self, reason: &str) {
        self.abort_requested.store(true, Ordering::SeqCst);
        *self.abort_reason.lock().unwrap_or_else(|p| p.into_inner()) = reason.to_string();
        self.abort.cancel();
    }
}

struct RunEntry {
    handle: RunHandle,
    finished_at: Option<Instant>,
}

/// Inner state guarded by the registry's single mutex: the forward run map
/// plus the `run_session` reverse index (run_id → (provider_bot_ref,
/// bcs_session_id)). Both are mutated under one lock so the lazy grace-TTL
/// sweep reclaims them atomically — `chat.abort`'s `find_terminal_run` never
/// observes a run_session entry whose run was already swept (and vice versa).
#[derive(Default)]
struct RunRegistryInner {
    map: HashMap<String, RunEntry>,
    run_session: HashMap<String, (String, String)>,
}

/// Registry of in-flight and recently-terminal runs, keyed by downstream body id.
///
/// `begin` is the create-or-get entry point: it atomically inserts a new run or
/// returns the existing handle for the same id (`is_new == false`). `get` reads
/// an existing handle without inserting. `finish` marks a run terminal and stamps
/// `finished_at` so the lazy sweep can reclaim it after [`TERMINAL_GRACE`].
///
/// `chat.abort` (Task 14) drives off two lookup paths:
/// - [`Self::get`] returns the active run's handle (so the abort handler can
///   cancel its token + invalidate its interactions).
/// - [`Self::find_terminal_run`] reverse-looks-up via the `run_session` index:
///   given `(provider_bot_ref, session_id)` it answers "is there a terminal run
///   recorded for this pair?" — the second leg of the abort response matrix
///   (terminal run → 410 `run_terminated`; no record → 200 `{"aborted": false}`).
#[derive(Default)]
pub struct RunRegistry {
    inner: Mutex<RunRegistryInner>,
}

impl RunRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn sweep_locked(inner: &mut RunRegistryInner) {
        let now = Instant::now();
        // Retain terminal entries within grace; always retain active.
        inner.map.retain(|_, e| {
            match e.finished_at {
                Some(t) => now.saturating_duration_since(t) < TERMINAL_GRACE,
                None => true,
            }
        });
        // Drop run_session entries whose runs were swept (the run no longer
        // lives in the map — the (bot, session) pair is no longer resolvable
        // by run id, so `find_terminal_run` must stop reporting it).
        inner.run_session.retain(|run_id, _| inner.map.contains_key(run_id));
    }

    /// Returns the existing handle for `run_id` (active or terminal), if any.
    /// Performs a lazy grace-TTL sweep of terminal entries.
    pub fn get(&self, run_id: &str) -> Option<RunHandle> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Self::sweep_locked(&mut inner);
        inner.map.get(run_id).map(|e| e.handle.clone())
    }

    /// Create a new run, or — if `run_id` already exists — return the existing
    /// handle. The second return is `true` iff a fresh run was created; `false`
    /// marks "same id already present" (re-attach / terminal-replay / conflict
    /// decision belongs to the caller, which compares [`RunHandle::matches`]).
    pub fn begin(&self, run_id: &str, fingerprint: String) -> (RunHandle, bool) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Self::sweep_locked(&mut inner);
        if let Some(entry) = inner.map.get(run_id) {
            return (entry.handle.clone(), false);
        }
        let (tx, _rx) = broadcast::channel::<String>(256);
        let handle = RunHandle {
            abort: CancellationToken::new(),
            tx,
            buffer: Arc::new(Mutex::new(Vec::new())),
            terminal: Arc::new(AtomicBool::new(false)),
            abort_requested: Arc::new(AtomicBool::new(false)),
            abort_reason: Arc::new(Mutex::new("user_cancelled".to_string())),
            fp: Arc::new(fingerprint),
        };
        inner.map.insert(
            run_id.to_string(),
            RunEntry { handle: handle.clone(), finished_at: None },
        );
        (handle, true)
    }

    /// Mark `run_id` terminal. Buffered frames are retained for [`TERMINAL_GRACE`]
    /// so a late re-send can replay the terminal state; lazy sweep reclaims them.
    pub fn finish(&self, run_id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = inner.map.get_mut(run_id) {
            entry.handle.terminal.store(true, Ordering::SeqCst);
            entry.finished_at = Some(Instant::now());
        }
    }

    /// Delete a run entry (and its `run_session` association), bypassing the
    /// grace-TTL retention [`Self::finish`] relies on. Rollback-only: the
    /// chat.send handler creates a placeholder entry via [`Self::begin`]
    /// before claiming the session slot (`try_start_run`), so it can roll
    /// back the placeholder on 429 (the session slot is held by a different
    /// run_id) instead of leaving a dangling never-spawned entry pinned in
    /// the registry for [`TERMINAL_GRACE`]. Removes from the main `map` then
    /// `run_session.retain` (one upsert), so a stale session association is
    /// never left pointing at a gone run_id.
    pub fn remove(&self, run_id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.map.remove(run_id);
        inner.run_session.retain(|rid, _| rid != run_id);
    }

    /// Record the `(provider_bot_ref, session_id)` association for `run_id`,
    /// enabling `chat.abort`'s [`Self::find_terminal_run`] reverse lookup.
    /// Called by `handle_chat_send` for a freshly-created run (right after
    /// [`Self::begin`] returns `is_new == true`). Idempotent on the same
    /// run_id — overwrites any stale association; stale entries are pruned
    /// by the grace-TTL sweep ([`Self::sweep_locked`]) once the run itself is
    /// swept.
    pub fn record_session(&self, run_id: &str, bot: &str, session: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .run_session
            .insert(run_id.to_string(), (bot.to_string(), session.to_string()));
    }

    /// Find a terminal run for `(bot, session)`, returning its run_id. Used by
    /// `chat.abort` to distinguish "no active run, but a terminal run was
    /// recorded for this session" (return 410 `run_terminated`) from "no record
    /// at all" (return 200 `{"aborted": false}`). Iterates the run_session
    /// reverse index and checks each candidate's `terminal` flag in the run
    /// map. There is at most one terminal run per session in practice: a fresh
    /// run cannot start while another is active (the session slot's 429 guard
    /// excludes it), so successive terminal runs for one session never overlap.
    pub fn find_terminal_run(&self, bot: &str, session: &str) -> Option<String> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Self::sweep_locked(&mut inner);
        inner
            .run_session
            .iter()
            .find(|(run_id, (b, s))| {
                b == bot && s == session
                    && inner.map.get(*run_id).map_or(false, |e| e.handle.is_terminal())
            })
            .map(|(run_id, _)| run_id.clone())
    }

    /// Abort every still-active run by calling [`RunHandle::request_abort`] with
    /// `reason` on each. Used by the provider's graceful shutdown path (Task 15):
    /// iterating in-flight runs and cancelling them lets each run loop finalize
    /// (engine killed, interactions invalidated, session slot released) instead
    /// of leaving orphaned drivers when the process exits. Terminal runs are
    /// skipped — they are already closing. The mutex is released before calling
    /// `request_abort` so the per-run cancellation (which writes the engine's
    /// abort token, not this registry) proceeds without holding the registry
    /// lock; cancellation itself is non-blocking.
    pub async fn abort_all(&self, reason: &str) {
        let handles: Vec<RunHandle> = {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            Self::sweep_locked(&mut inner);
            inner
                .map
                .values()
                .filter(|e| !e.handle.is_terminal())
                .map(|e| e.handle.clone())
                .collect()
        };
        for handle in handles {
            handle.request_abort(reason);
        }
    }
}

/// Idempotency fingerprint for a chat.send body: `message` + `session_id` +
/// `to_bot.provider_bot_ref`, joined by the ledger's unit separator. `message`
/// is serialized via `serde_json` so structurally-equal JSON compares equal
/// regardless of key ordering; `to_string` failing degrades to an empty string
/// (the body is deserialized upstream, so failure is not expected in practice).
pub fn body_fingerprint(req: &DownstreamRequest, session_id: &str) -> String {
    let msg = serde_json::to_string(&req.message)
        .unwrap_or_else(|_| String::new());
    crate::idempotency::fingerprint(&[&msg, session_id, &req.to_bot.provider_bot_ref])
}

/// Prompt assembly for a turn: pending injects (drained FIFO) prepend each as
/// `[from:{name}] {text}` (or bare `{text}` when `from_name` is `None`), then a
/// blank separator, then the user message text — `message.content[].text`
/// joined with `\n`. Two injects + body thus render as:
///
/// ```text
/// [from:张三] 注入的消息一
/// 注入的消息二
///
/// <本次 message 文本>
/// ```
///
/// The blank line marks where the inject block ends and the current request
/// begins — visible separation that downstream prompts read as context vs ask.
/// The pending-inject prepend is the codex fallback (no transcript sink): an
/// inject that could not be sunk to the engine transcript lives in
/// `pending_injects` and is drained here on the next chat.send. UTF-8 safe —
/// no byte slicing.
async fn assemble_prompt(
    state: &AppState,
    bot: &BotConfig,
    session_id: &str,
    req: &DownstreamRequest,
) -> String {
    let injects = state
        .sessions
        .take_pending_injects(&bot.provider_bot_ref, session_id)
        .await;
    let mut prefix = String::new();
    for inj in &injects {
        if !prefix.is_empty() {
            prefix.push('\n');
        }
        match &inj.from_name {
            Some(name) => prefix.push_str(&format!("[from:{name}] {}", inj.text)),
            None => prefix.push_str(&inj.text),
        }
    }
    let body = extract_message_text(req.message.as_ref());
    if prefix.is_empty() {
        body
    } else {
        format!("{prefix}\n\n{body}")
    }
}

/// Extract `message.content[].text` and join multiple parts with `\n`. Missing
/// fields yield an empty string (validated upstream). Reused by the chat.inject
/// handler to flatten the inject body into the [`crate::session::InjectedMessage`]
/// text field, so the pending-prepend and transcript-sink paths see one string.
pub(crate) fn extract_message_text(message: Option<&serde_json::Value>) -> String {
    let Some(msg) = message else { return String::new() };
    let Some(content) = msg.get("content").and_then(|c| c.as_array()) else {
        return String::new();
    };
    let texts: Vec<&str> = content
        .iter()
        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
        .collect();
    texts.join("\n")
}

/// Drive the engine turn and push frames into the run's buffer+broadcast. Runs
/// until a terminal condition (final / engine-EOF / deadline / disconnect),
/// then marks the run terminal, releases the session slot, and notifies the
/// registry. The forward stream to the client is consumed separately via
/// [`forward_stream`].
async fn run_driver(
    state: Arc<AppState>,
    handle: RunHandle,
    req: DownstreamRequest,
    bot: BotConfig,
    session_id: String,
) {
    let run_id = req.id.clone();
    let timeout_ms = req.timeout_ms.unwrap_or(3_600_000);

    // Resume an established engine-internal session if one was recorded.
    let engine_session_id = state
        .sessions
        .mapping(&bot.provider_bot_ref, &session_id)
        .await
        .engine_session_id;

    let prompt = assemble_prompt(&state, &bot, &session_id, &req).await;

    let turn_req = TurnRequest {
        run_id: run_id.clone(),
        prompt,
        engine_session_id,
        cwd: bot.cwd.clone(),
        model: bot.model.clone(),
        cfuse_bin: bot.cfuse_bin.clone().unwrap_or_else(|| PathBuf::from("cfuse")),
        permission_mode: bot.permission_mode.clone(),
        interactions: state.interactions.clone(),
        trace: state.trace.as_ref().map(|store| {
            TraceContext::new(
                store.clone(),
                match bot.engine {
                    crate::config::EngineKind::CfuseCc => "cfuse-cc",
                    crate::config::EngineKind::CfuseCodex => "cfuse-codex",
                },
                run_id.clone(),
            )
        }),
    };

    let (ev_tx, mut ev_rx) = mpsc::channel::<StreamEvent>(64);
    let trace = turn_req.trace.clone();
    let abort_token = handle.abort.clone();
    let engine = build_engine(&bot);
    let mut engine_handle: Option<tokio::task::JoinHandle<Result<TurnOutcome, TurnError>>> =
        Some(tokio::spawn(async move {
            engine.run_turn(turn_req, ev_tx, abort_token).await
        }));

    let mut seq: u64 = 0;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Self-terminate ~30s ahead of the hard deadline so a terminal chat_error can
    // still flush before the client times out.
    let deadline_ms = timeout_ms.saturating_sub(30_000);
    let deadline = tokio::time::sleep(Duration::from_millis(deadline_ms));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                // Deadline: emit terminal chat_error(deadline) and close.
                let _ = push_frame(&handle, &mut seq, &run_id,
                    &sse::chat_error(&run_id, "run deadline exceeded", Some("deadline")),
                    trace.as_ref());
                break;
            }
            _ = heartbeat.tick() => {
                // Heartbeat is a raw comment frame; no seq, no encode.
                if !push_raw(&handle, HEARTBEAT) {
                    break;
                }
            }
            ev = ev_rx.recv() => {
                match ev {
                    Some(StreamEvent::Chat(c)) if c.state == ChatState::Final => {
                        let _ = push_frame(
                            &handle,
                            &mut seq,
                            &run_id,
                            &StreamEvent::Chat(c),
                            trace.as_ref(),
                        );
                        break;
                    }
                    Some(event) => {
                        match push_frame(&handle, &mut seq, &run_id, &event, trace.as_ref()) {
                            PushOutcome::Ok => {}
                            PushOutcome::Disconnect | PushOutcome::Terminate => break,
                        }
                    }
                    None => {
                        // Engine task ended without emitting a Chat(Final) event
                        // (the cc/codex drivers return the final text via
                        // TurnOutcome). Resolve the outcome and emit the terminal
                        // frame here. `Aborted` is silent (handled by the
                        // post-loop cancel).
                        let outcome: Result<TurnOutcome, TurnError> = match engine_handle.take() {
                            Some(h) => match h.await {
                                Ok(r) => r,
                                Err(join_err) => Err(TurnError::EngineExited(format!("task join: {join_err}"))),
                            },
                            None => Err(TurnError::EngineExited("engine task missing".into())),
                        };
                        match outcome {
                            Ok(o) => {
                                if let Some(sid) = o.engine_session_id {
                                    state.sessions
                                        .set_engine_session_id(&bot.provider_bot_ref, &session_id, &sid)
                                        .await;
                                }
                                match o.final_text {
                                    Some(text) => {
                                        let _ = push_frame(&handle, &mut seq, &run_id,
                                            &sse::chat_final(&run_id, text),
                                            trace.as_ref());
                                    }
                                    None => {
                                        let _ = push_frame(&handle, &mut seq, &run_id,
                                            &sse::chat_error(&run_id,
                                                "engine exited without final text",
                                                Some("runtime_error")),
                                            trace.as_ref());
                                    }
                                }
                            }
                            Err(TurnError::Aborted) => {
                                // Explicit abort (chat.abort or graceful
                                // shutdown) → emit a terminal `chat_aborted`
                                // frame so the BCS SSE consumer sees the final
                                // `state=aborted` (spec §5.3). A passive BCS
                                // disconnect (push_raw's broadcast send returns
                                // no subscribers) does NOT set the abort flag —
                                // its run closes silently, the stream just ends.
                                if handle.is_abort_requested() {
                                    let _ = push_frame(
                                        &handle,
                                        &mut seq,
                                        &run_id,
                                        &sse::chat_aborted(&run_id, &handle.abort_stop_reason()),
                                        trace.as_ref(),
                                    );
                                }
                            }
                            Err(e) => {
                                let _ = push_frame(&handle, &mut seq, &run_id,
                                    &sse::chat_error(&run_id, &e.to_string(),
                                        Some("runtime_error")),
                                    trace.as_ref());
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // Finalize: release any parked HITL interactions with a deny fallback
    // (spec §6.3: deadline → safe fallback; abort → deny) so the driver's
    // resolution_rx never blocks on a dead receiver. Done BEFORE cancelling
    // the engine so the fallback is delivered through the resolution channel
    // rather than lost to a dropped receiver; entries are retained (marked
    // resolved) so a late BCS resolve replays as Duplicate instead of Unknown.
    state
        .interactions
        .invalidate_run(&run_id, json!({ "decision": "deny" }));

    // Cancel the engine (idempotent), await its task if we did not already,
    // then mark terminal, release the session slot, notify registry.
    handle.abort.cancel();
    if let Some(h) = engine_handle.take() {
        let _ = h.await;
    }
    handle.terminal.store(true, Ordering::SeqCst);
    state.runs.finish(&run_id);
    state
        .sessions
        .finish_run(&bot.provider_bot_ref, &session_id, &run_id)
        .await;
}

/// Push one engine event as a frame into buffer+broadcast. On oversize
/// ([`FrameError::FrameTooLarge`]) emit a terminal `chat_error` instead and
/// signal termination — never emit an oversize frame.
///
/// Synchronous (no `.await`) — the buffer mutex is never held across an await.
fn push_frame(
    handle: &RunHandle,
    seq: &mut u64,
    run_id: &str,
    ev: &StreamEvent,
    trace: Option<&TraceContext>,
) -> PushOutcome {
    *seq += 1;
    let ts = now_ms();
    let frame = match event_to_frame(ev, *seq, ts, run_id) {
        Ok(f) => f,
        Err(FrameError::FrameTooLarge(_)) => {
            // Emit a bounded terminal error instead of the oversize frame.
            *seq += 1;
            let err_ev = sse::chat_error(run_id, "frame too large", Some("runtime_error"));
            let err_frame = match event_to_frame(&err_ev, *seq, now_ms(), run_id) {
                Ok(f) => f,
                Err(_) => return PushOutcome::Terminate,
            };
            let _ = push_raw(handle, &err_frame);
            return PushOutcome::Terminate;
        }
        Err(_) => return PushOutcome::Terminate,
    };
    if let Some(trace) = trace {
        trace.record_converted(ev, *seq);
        trace.record_sse(*seq, &frame);
    }
    if push_raw(handle, &frame) {
        PushOutcome::Ok
    } else {
        PushOutcome::Disconnect
    }
}

/// Append a pre-formatted frame string to the buffer and broadcast it. The
/// buffer write + broadcast send happen under the buffer mutex so that a
/// concurrent forwarder's `(subscribe, snapshot)` observes the two as one atomic
/// operation — neither duplicated nor lost. Returns `false` if the broadcast had
/// no live subscribers (BCS disconnect).
fn push_raw(handle: &RunHandle, frame: &str) -> bool {
    let send_ok = {
        let mut buf = handle.buffer.lock().unwrap_or_else(|p| p.into_inner());
        buf.push(frame.to_string());
        handle.tx.send(frame.to_string())
    };
    send_ok.is_ok()
}

/// Build the client-facing SSE response: spawn a forwarder that replays the
/// buffered frames then follows the broadcast, and wrap its mpsc receiver as a
/// `Body::from_stream`. This is the re-attach path too — the same handle is
/// reused, so a second subscriber replays the buffer and joins the live stream.
///
/// `subscribe()` + buffer snapshot are taken atomically (under the buffer
/// mutex) so the snapshot's contents exactly partition from the broadcast's
/// post-subscribe messages — no duplicate frames, no lost frames. The forwarder
/// then drains the broadcast until the run is terminal and the receiver is
/// empty; a short poll wakes it after the driver marks terminal so it exits
/// promptly (broadcast `Closed` never fires because the registry retains the
/// `Sender` for terminal replay).
pub fn forward_stream(handle: RunHandle) -> impl Stream<Item = String> + Send + 'static {
    let (tx, rx) = mpsc::channel::<String>(64);
    // Atomic (w.r.t. pushes): snapshot the buffer and subscribe so the partition
    // is exact — snapshot holds frames pushed up to here; broadcast carries only
    // frames pushed after subscribe.
    let (snapshot, subscriber) = {
        let buf = handle.buffer.lock().unwrap_or_else(|p| p.into_inner());
        let snap = buf.clone();
        let sub = handle.tx.subscribe();
        (snap, sub)
    };
    tokio::spawn(async move {
        // Replay the buffer snapshot first.
        for frame in snapshot {
            if tx.send(frame).await.is_err() {
                return;
            }
        }
        // Then follow the live broadcast until terminal + drained.
        let mut sub = subscriber;
        loop {
            tokio::select! {
                ev = sub.recv() => {
                    match ev {
                        Ok(frame) => {
                            if tx.send(frame).await.is_err() {
                                return;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                target: "bridge_provider",
                                n, "SSE broadcast lagged; BCS tolerates seq gaps"
                            );
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
                _ = tokio::time::sleep(TERMINAL_POLL) => {
                    // Driver marked terminal: drain any remaining buffered
                    // broadcast messages, then stop.
                    if handle.is_terminal() {
                        loop {
                            match sub.try_recv() {
                                Ok(frame) => {
                                    if tx.send(frame).await.is_err() {
                                        return;
                                    }
                                }
                                Err(broadcast::error::TryRecvError::Empty)
                                | Err(broadcast::error::TryRecvError::Closed) => return,
                                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                            }
                        }
                    }
                }
            }
        }
    });
    tokio_stream::wrappers::ReceiverStream::new(rx)
}

/// Wrap a frame stream as an SSE `text/event-stream` response.
pub fn sse_response(stream: impl Stream<Item = String> + Send + 'static) -> Response {
    let body = Body::from_stream(
        stream.map(|s| Ok::<_, Infallible>(Bytes::from(s))),
    );
    let mut resp = Response::new(body);
    resp.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    resp.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

/// Spawn the run-driver task for a freshly-created run and return the SSE
/// response streaming its frames. The caller must have already reserved the
/// session slot and confirmed `is_new == true` via [`RunRegistry::begin`].
pub fn spawn_run(
    state: Arc<AppState>,
    req: DownstreamRequest,
    bot: BotConfig,
    session_id: String,
    handle: RunHandle,
) -> Response {
    // Build the forward stream BEFORE spawning the driver. `forward_stream`
    // subscribes to the broadcast synchronously inside the call; the driver's
    // heartbeat interval first-ticks immediately, and a broadcast send with no
    // live receiver reads as a BCS disconnect (`push_raw` returns false → the
    // run breaks as a false-positive disconnect). Subscribing first guarantees a
    // receiver exists before the driver can send its first heartbeat.
    let stream = forward_stream(handle.clone());
    let driver_handle = handle;
    tokio::spawn(async move {
        run_driver(state, driver_handle, req, bot, session_id).await;
    });
    sse_response(stream)
}

/// Re-attach a terminal run's buffered frames as a fresh one-shot SSE stream
/// (no driver, no broadcast subscription — the run is already closed).
pub fn terminal_replay_response(handle: RunHandle) -> Response {
    let (tx, rx) = mpsc::channel::<String>(64);
    let h = handle;
    tokio::spawn(async move {
        let snapshot = h
            .buffer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for frame in snapshot {
            if tx.send(frame).await.is_err() {
                return;
            }
        }
    });
    sse_response(tokio_stream::wrappers::ReceiverStream::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_new_then_existing_returns_same_handle() {
        let reg = RunRegistry::new();
        let (h1, is_new) = reg.begin("r-1", "fp-a".into());
        assert!(is_new);
        let (h2, is_new2) = reg.begin("r-1", "fp-a".into());
        assert!(!is_new2);
        assert!(h2.matches("fp-a"), "existing handle carries the same fingerprint");
        assert!(h1.matches("fp-a"));
        assert!(!h1.matches("fp-b"));
        assert!(reg.get("r-1").is_some());
        assert!(reg.get("r-2").is_none());
    }

    #[test]
    fn finish_marks_terminal_and_retains_for_replay() {
        let reg = RunRegistry::new();
        let (h, _) = reg.begin("r-9", "fp".into());
        assert!(!h.is_terminal());
        reg.finish("r-9");
        assert!(h.is_terminal());
        assert!(reg.get("r-9").is_some(), "terminal run retained within grace TTL");
    }

    #[test]
    fn body_fingerprint_stable_for_same_body_distinct_for_different() {
        let mk = |msg: serde_json::Value, sid: &str, ref_: &str| {
            let req = DownstreamRequest {
                id: "x".into(),
                method: "chat.send".into(),
                to_bot: crate::webhook::ToBot { provider_id: "p".into(), provider_bot_ref: ref_.into() },
                session_id: Some(sid.into()),
                message: Some(msg),
                from: None,
                timeout_ms: None,
                params: None,
            };
            body_fingerprint(&req, sid)
        };
        let m = serde_json::json!({"role":"user","content":[{"type":"text","text":"hi"}]});
        let a = mk(m.clone(), "s-1", "b-1");
        let b = mk(m.clone(), "s-1", "b-1");
        assert_eq!(a, b, "same body → same fingerprint");
        let c = mk(m.clone(), "s-1", "b-2");
        assert_ne!(a, c, "different ref → different fingerprint");
        let d = mk(serde_json::json!({"role":"user","content":[{"type":"text","text":"yo"}]}), "s-1", "b-1");
        assert_ne!(a, d, "different message → different fingerprint");
    }

    #[test]
    fn extract_message_text_joins_multiple_content_blocks() {
        let m = serde_json::json!({"content":[
            {"type":"text","text":"line1"},
            {"type":"image","text":"ignored"},  // non-text type but text present: still joined
            {"type":"text","text":"line2"},
            {"type":"text"},                     // no text: skipped
        ]});
        assert_eq!(extract_message_text(Some(&m)), "line1\nignored\nline2");
        assert_eq!(extract_message_text(None), "");
    }

    #[test]
    fn request_abort_sets_flag_overrides_reason_and_cancels_token() {
        // A fresh run handle's flag is false and stop_reason defaults to
        // "user_cancelled"; calling request_abort flips the flag, overrides the
        // stop_reason, and cancels the CancellationToken.
        let reg = RunRegistry::new();
        let (h, _) = reg.begin("r-a", "fp".into());
        assert!(!h.is_abort_requested(), "fresh run is not aborted");
        assert_eq!(h.abort_stop_reason(), "user_cancelled");

        let r2 = h.clone();
        h.request_abort("provider_shutdown");
        assert!(h.is_abort_requested(), "flag set after request_abort");
        assert_eq!(h.abort_stop_reason(), "provider_shutdown", "reason overridden");
        assert!(r2.is_abort_requested(), "shared flag visible to cloned handle");
        assert_eq!(r2.abort_stop_reason(), "provider_shutdown");
        // Cancellation propagates to all clones (CancellationToken is shared).
        assert!(h.abort.is_cancelled(), "token cancelled after request_abort");
    }

    #[test]
    fn find_terminal_run_returns_terminal_match_none_for_active_or_unknown() {
        // Active run (not terminal): find_terminal_run returns None — abort
        // goes through the sessions.active_run path instead.
        let reg = RunRegistry::new();
        let (_h_active, _) = reg.begin("r-active", "fp".into());
        reg.record_session("r-active", "bot-1", "s-1");
        assert_eq!(reg.find_terminal_run("bot-1", "s-1"), None, "active run is not terminal");

        // Mark it terminal: now find_terminal_run resolves to its run_id.
        reg.finish("r-active");
        assert_eq!(reg.find_terminal_run("bot-1", "s-1").as_deref(), Some("r-active"));

        // Unknown session and bot mismatch: no match.
        assert_eq!(reg.find_terminal_run("bot-1", "s-unknown"), None, "unknown session");
        assert_eq!(reg.find_terminal_run("bot-other", "s-1"), None, "bot mismatch");
    }

    #[test]
    fn remove_drops_entry_and_run_session_association_for_rollback() {
        // chat.send's 429 rollback path: the placeholder entry created by
        // `begin` (and its record_session association, if any) must be wiped
        // so the same run_id is not pinned in the registry and a same-id retry
        // is not later surprised by a stale terminal-replay entry. `remove`
        // bypasses the grace-TTL retention `finish` relies on.
        let reg = RunRegistry::new();
        let (_h, is_new) = reg.begin("r-roll", "fp".into());
        assert!(is_new);
        reg.record_session("r-roll", "bot-1", "s-1");
        assert!(reg.get("r-roll").is_some());

        reg.remove("r-roll");

        assert!(reg.get("r-roll").is_none(), "entry removed by rollback");
        assert_eq!(reg.find_terminal_run("bot-1", "s-1"), None,
            "run_session association pruned alongside the entry");
    }

    #[tokio::test]
    async fn abort_all_cancels_every_active_run_and_skips_terminal() {
        // Two active + one terminal: only the two active handles have their
        // abort tokens cancelled after abort_all; the terminal one stays as-is
        // (it was already cancelled when its run loop finalized).
        let reg = RunRegistry::new();
        let (h_a, _) = reg.begin("r-a", "fp".into());
        let (h_b, _) = reg.begin("r-b", "fp".into());
        let (h_t, _) = reg.begin("r-t", "fp".into());
        reg.finish("r-t");
        assert!(!h_a.abort.is_cancelled());
        assert!(!h_b.abort.is_cancelled());

        reg.abort_all("provider_shutdown").await;

        assert!(h_a.abort.is_cancelled(), "active run a cancelled");
        assert!(h_b.abort.is_cancelled(), "active run b cancelled");
        assert!(!h_t.abort.is_cancelled(), "terminal run skipped");
        assert!(h_a.is_abort_requested() && h_b.is_abort_requested(), "flag set on each");
        assert_eq!(h_a.abort_stop_reason(), "provider_shutdown");
    }
}
