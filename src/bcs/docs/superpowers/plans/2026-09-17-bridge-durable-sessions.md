# Bridge durable sessions and inject delivery

> **For agentic workers:** Use superpowers:executing-plans for the coupled store/runtime changes; delegate the independent process-level recovery tests using superpowers:subagent-driven-development.

**Goal:** Resume engine sessions after Bridge restart and durably queue every inject until a successful send consumes it, removing native transcript mutation entirely.

**Architecture:** A Bridge-owned SQLite database is the authoritative store for session IDs and inject receipts. Engine adapters report validated session IDs through an internal observer immediately; runs reserve inject batches and acknowledge them only after successful completion. Active processes, permissions and SSE buffers remain process-local.

**Tech Stack:** Rust, existing workspace rusqlite, Tokio, SQLite, local mock CLI integration tests.

**Spec:** `../specs/2026-08-31-bridge-provider-design.md`, updated as part of this change; user-approved durable queue design in this conversation.

## Constraints and decisions

- No global formatting, no native CC history reads/writes, no real model calls in tests.
- Prior authorization/result-envelope changes were committed and pushed separately as `8e73d999f`, hooks skipped as requested. New implementation stays reviewable in the working tree.
- `state_path` defaults to `~/.bcn-bridge/bridge-state.sqlite3` (user-requested follow-up); explicit `~/` expands to the user home and ordinary relative paths resolve beside the config. Database open/migration/write failures must propagate; no memory fallback.
- Database keys include provider, bot, BCS session; session rows bind engine kind and canonical cwd. Changing that identity rejects resume instead of silently creating another history. Engine-native histories must remain under the same runtime account.
- Hold an OS file lock for the lifetime of the store: one Bridge process per database. Run exclusion remains in memory and never survives a process restart.
- Delivery is at least once: reserve `pending -> inflight(run_id)`, commit `inflight -> delivered` before final SSE, return failures to pending, recover orphaned inflight rows on startup. Ambiguous engine receipt may repeat context on the next send; no automatic model invocation and no exactly-once claim.
- Durable inject receipts compare the full message/from payload using canonical JSON serialization. Preserve receipts after delivery so late retries do not redeliver.

## Task 1: Durable store and configuration

Files: `src/session.rs`, new `src/session_db.rs`, `src/config.rs`, `src/webhook.rs` bootstrap, `src/main.rs`, crate Cargo manifest/lock.

- [x] Add tests for reopen/resume, provider/bot/session isolation, metadata mismatch, duplicate/conflicting inject IDs, FIFO reservation and next-run isolation, failure/restart redelivery, durable delivered receipts, write failures and exclusive ownership.
- [x] Run the tests and observe missing persistence before implementation.
- [x] Add SQLite tables/schema version and file lock; run database operations off the Tokio executor using `spawn_blocking`. Keep active-run locks separate.
- [x] Expose `mapping`, `set_engine_session_id`, `enqueue_inject`, `claim_injects`, `complete_injects`, `release_injects` as fallible async store operations. Configuration load resolves a stable state path; startup returns errors rather than constructing an empty store.
- [x] Verify `cargo test -p bridge-provider --lib session` and config tests.

## Task 2: Engine lifecycle and queue consumption

Files: `src/engine/mod.rs`, CC/Codex drivers, `src/run.rs`, `src/webhook.rs`, remove `src/engine/transcript.rs`, update existing integration helpers/tests.

- [x] Add/observe failing process tests for restart resume, early ID capture, inject persistence and redelivery (independent test agent).
- [x] Introduce a required internal session observer; persist at CC init / Codex thread creation or resume before proceeding. Storage errors abort the turn. CC `is_error:true` results cannot consume injects as successful turns.
- [x] Reserve pending messages before constructing the prompt, prepend FIFO, acknowledge only successful outcomes, release after cancellation/failure. Persist session ID before final independently of outcome.
- [x] Replace inject handler with transactional SQLite receipt/enqueue, ACK only after commit; remove transcript module, old sink tests and HOME-mutating test utilities.
- [x] Add safe tracing of provider/bot/BCS session/run/engine session and new/resume choice.

## Task 3: Recovery conformance and documentation

Files: new `tests/session_recovery.rs`, new mock CLI fixture, existing design specification.

- [x] Exercise real Bridge process restart with the same SQLite path and synthetic engine; verify resume, queued receipt dedupe, failed/inflight recovery, no spontaneous model invocation.
- [x] Document configuration, lifecycle, at-least-once boundary, old-history migration limits, and removal of transcript writes.
- [x] Run Bridge tests (known environment pipe-size failures reported separately if still present), build, diff checks and a focused code review. Resolve material findings before completion.

## Progress

- Initial plan: implementation authorized; prior work push confirmed. Database/runtime changes implemented locally; independent process tests delegated and passing (6 cases). Store tests passing (13 cases); final validation/review pending.

- Final validation: `cargo test -p bridge-provider --no-fail-fast` ran 131 tests: 129 passed; only the two pre-existing CLI pipe-capacity tests failed (host pipe remains 4096 bytes, long fixture expects 7068). All 14 store tests, 8 process recovery/fault tests, 21 webhook tests and 14 golden tests passed.
- Review fixes: canonicalize database paths before deriving owner lock (symlink alias RED → GREEN); batch completion/release use explicit transactions (second-row SQLite failure RED → GREEN). Scoped re-review found no remaining findings. Original `.gitignore` rules preserved, SQLite rules appended.
- Existing native histories are untouched. Prior in-memory mappings are not automatically imported; first deployment must establish a durable mapping before restart continuity applies.

- User follow-up: default state moved to `~/.bcn-bridge/bridge-state.sqlite3`; explicit state paths still override it. Home-directory creation and restart recovery tested in isolated child-process HOME directories; no global environment mutation or production database migration.
- Follow-up validation: 21 webhook tests, 9 process recovery tests, 2 config tests, and the AppState recreation test passed (33 total); the new home-path test was first observed failing against the old config-directory default.
