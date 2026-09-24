# Bridge WebSocket V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add outgoing V2 Bot WebSockets to Bridge while preserving gateway behavior and running cfuse through network disconnections.

**Architecture:** Both adapters call the same runtime and encode structured RunEvent records. GatewayEncoder and WebSocketV2Encoder each implement encode/decode. SQLite holds engine session/inject state and scoped reconnect credentials.

**Tech Stack:** Rust, Tokio, tokio-tungstenite, serde, existing bcs-protocol, rusqlite.

**Spec:** ../specs/2026-09-18-bridge-upstream-v2-design.md

## Global Constraints

- No BCS WS authorization changes; this belongs to V3.
- Upstream disconnection preserves cfuse and buffers unsent events, including final.
- Gateway retains HTTP/SSE contracts and disconnect cancellation.
- Preserve ~/.bcn-bridge/bridge-state.sqlite3 and existing engine-session mappings.
- Database writes fail visibly. No real Bot messages or model calls in tests.
- No cargo fmt or unrelated formatting. Existing unrelated scripts remain untouched.
- Work in the existing isolated bcn-bridge-gateway worktree.

## Task 1: Configuration and reconnect identity persistence

Files: config.rs, session.rs, session_db.rs, session_tests.rs, tests/support/mod.rs.

Interfaces: ProviderConfig.mode (ConnectionMode::Gateway/Upstream), optional
gateway listen/token fields validated at load, ProviderConfig.upstream with
url/reconnect_interval_ms/heartbeat_interval_ms/connect_timeout_ms. BotConfig
adds optional bot_id/token used only by upstream. SessionStore exposes
load_identity(server, bot_ref) and save_identity(server, bot_ref, identity),
where ConnectionIdentity contains bot_id and token. Existing provider_id and
provider_bot_ref keep stable durable namespaces.

- [x] Add failing config tests using an upstream document without listen/token, invalid URL, duplicate Bot identity, and a legacy gateway document.
- [x] Run `cargo test -p bridge-provider config::tests --lib`; observe rejection of the new document before implementing mode support.
- [x] Implement validated mode selection and a transactional SQLite v1 migration for scoped reconnect identities.
- [x] Add and run persistence tests: save/reopen identity; different server/local Bot cannot read it; failed writes never report success.

Example acceptance assertion:
```rust
assert_eq!(config.mode, ConnectionMode::Upstream);
assert!(config.listen.is_none());
assert_eq!(loaded_identity.bot_id, "bot-a");
```

## Task 2: Shared runtime and structured event delivery

Files: new runtime.rs and encoder.rs; run.rs, webhook.rs, error.rs,
idempotency.rs, lib.rs; new tests/encoder_contract.rs.

Interfaces: normalized CommandRequest; RuntimeReply (ordinary reply or run
handle); RunEvent {run_id,seq,ts,event:StreamEvent}; Encoder with associated
input/output types and encode/decode. Webhook remains the gateway adapter.

- [x] Add contract tests for identical SSE metadata on replay and preserving structured tool-result JSON.
- [x] Observe the new structured subscription test fail against the SSE-only buffer.
- [x] Extract business request handling out of webhook without changing wire responses; move protocol headers/auth into the gateway adapter.
- [x] Change RunHandle to buffer RunEvent records, provide subscriber-safe replay, and keep gateway heartbeat encoding in the gateway output stream.
- [x] Add per-run transport policy: gateway disconnect cancels; upstream event delivery can detach while execution continues; V2 interaction fails explicitly.
- [x] Run existing golden_frames, e2e_webhook and session_recovery suites.

Example acceptance assertions:
```rust
assert_eq!(replayed.seq, original.seq);
assert_eq!(replayed.ts, original.ts);
assert_eq!(ws_frame["event"], "agent");
assert_eq!(ws_frame["payload"]["data"]["result"]["value"], 7);
```

## Task 3: WebSocket V2 adapter and connection-independent runs

Files: new upstream.rs (split connection IO from run event delivery if needed),
encoder.rs, main.rs, Cargo.toml/Cargo.lock; tests/upstream_websocket.rs and
tests/fixtures/mock_cc_upstream.py.

Interfaces: upstream::serve(Arc<AppState>, CancellationToken); each Bot owns
one client and pending run cursors, calls Runtime, and encodes V2 frames.

- [x] Write a loopback integration test that sends a run, closes its socket, allows the engine to finish offline, reconnects and expects the original final.
- [x] Observe failure before adding upstream transport.
- [x] Implement bot.connect/identity validation/persistence, heartbeat, ordered writer, bounded IO waits and reconnection; preserve runs across connections.
- [x] Implement send/inject/abort request mapping and ACK ordering. Retain unsent terminal events until delivery; never restart the engine on reconnect.
- [x] Implement bounded offline event retention, explicit unsupported V2 interaction error, kick handling and shutdown cancellation.
- [x] Wire main mode selection; keep default gateway startup unchanged.
- [x] Run loopback tests with at least these literal assertions:
```rust
assert_eq!(ack["id"], "request-1");
assert_eq!(ack["payload"]["run_id"], "run-1");
assert_eq!(final_frame["payload"]["run_id"], "run-1");
assert_eq!(engine_start_count, 1);
assert_eq!(reconnected_bot_id, "bot-a");
```

## Task 4: Integration verification and documentation

Files: both design docs, this plan, gateway design compatibility notes.

- [x] Document validated gateway/upstream configuration examples and V2 delivery limits.
- [x] Run `cargo test -p bridge-provider --no-fail-fast`; report any environment limitations separately from regressions.
- [x] Run `cargo build -p bridge-provider` and `git diff --check`.
- [x] Review the complete diff for transport coupling, wrong Bot/session routing, lost offline final, duplicate engine runs and unreported persistence errors.
- [x] Resolve findings and leave the final source ready for user review; do not publish or send real BCS messages.

## Execution record

- 2026-09-18: Existing linked worktree confirmed; no source changes before baseline.
- Baseline excludes the two previously established host pipe-size failures; all other tests will be rerun. Full final run will report these limitations explicitly.

- Configuration/migration: 9 config + 20 session tests passed; task review passed.
- Shared runtime: typed replay and FIFO delivery tests passed; first review required removing gateway-specific sizing/heartbeats. Fix wave moved them to the gateway adapter and added explicit encoding-error termination.
- WS loopback: initial six integration tests passed; deeper tests exposed shutdown identity-commit cancellation, now removed. A parallel fork/file-lock lifetime edge is under deterministic investigation.
- Protocol contract tests now consume actual BCS V2 frame builders and deserialize emitted frames into BCS consumer DTOs.
- Full architecture runner reported failures in existing unmodified BCS dependency/import/conformance checks. Stopped its workspace-wide test enumeration to finish focused Bridge validation; log: /tmp/bridge-upstream-arch.log.

- Final full crate run after fixes: `cargo test -p bridge-provider --no-fail-fast` exited 0: 175 passed (112 unit, 21 gateway, 7 encoder, 14 golden, 2 runtime, 9 session recovery, 10 upstream), no skipped tests. The two earlier host pipe-size tests also passed in this run.
- Owner-lock inheritance race reproduced with a paused pre-exec child, then fixed by explicit unlock after SQLite closes. Deterministic regression and all 21 session tests passed.
- Upstream retention now clears delivered terminal payloads and starts a fresh, nonrenewing five-minute tombstone TTL when delivery completes; long offline completion cannot immediately lose idempotency.

- Final whole-change review: one Important issue found in retained full-message fingerprints. Fixed-size, length-delimited SHA-256 fingerprints were added; scoped re-review marked the issue addressed with no new Important/Critical findings. No Critical findings; other required behavior passed review.
- `cargo build -p bridge-provider` and `git diff --check` passed before the final fingerprint fix.

- Final fingerprint fix added three regressions. Full final run: 176 passed, two unchanged CLI pipe-capacity tests failed with a 4096-byte pipe (expected expanded pipe and complete 7068-byte JSONL). All 10 WS and all gateway/recovery/contract tests passed. Earlier complete run of 175 tests passed without skips; host pipe capacity varies. No engine CLI code was changed.
- Final `cargo build -p bridge-provider` and `git diff --check`: passed. Code remains in the existing worktree for review; no new commit/push or live BCS/model calls.

- Focused final CLI recheck: `cargo test -p bridge-provider --lib engine::cli::tests -- --test-threads=1` passed all four tests, including both pipe-capacity failures. Final source is unchanged between the full run and this recheck. All 178 test cases have passing evidence; the final parallel full run itself remains recorded as 176 pass / 2 transient host pipe failures.
