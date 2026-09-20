# Bridge WebSocket V2 upstream design

## Approved scope

Bridge supports two selectable connection modes: the existing Provider 2.0
HTTP/SSE gateway and a new outgoing Bot WebSocket V2 connection. Both use one
runtime, the existing cfuse drivers, durable engine-session mappings and inject
queue. Each mode has an Encoder implementing encode/decode. Adapters own IO,
authentication and connection lifecycle; encoders own wire translation.

User decision: do not implement BCS WebSocket interaction authorization in this
change. That belongs to V3. Gateway authorization remains available. Upstream
V2 reports a terminal unsupported-interaction error when an engine requires
interactive authorization; it never silently approves or waits indefinitely.

User decision: network loss in upstream mode does not cancel cfuse. Keep the
original run/session, retain unsent events (including final), reconnect as the
same Bot, finish bot.connect, and drain events in order before live delivery.
Do not re-execute chat.send to recover a connection. Gateway retains its existing
SSE-disconnect cancellation behavior.

## Boundaries and data flow

```text
BCS HTTP -> Gateway adapter -> GatewayEncoder.decode -----+
                                                       |
BCS WS <-> Upstream adapter -> WebSocketV2Encoder.decode -+-> Runtime
                                                           |
                                           SessionStore / RunRegistry
                                                           |
                                                     cfuse driver
                                                           |
                                                       StreamEvent
                                                           |
                                             RunEvent {run_id,seq,ts,event}
                                                           |
                    GatewayEncoder.encode <----------------+--> WS encoder
                           |                                      |
                          SSE                                 WS V2 frames
```

Runtime accepts normalized requests, returns an ordinary reply or a run handle,
and never creates an HTTP response or owns a socket. Gateway wire DTOs keep
their old shape. The shared error/result representation does not depend on
axum. WebSocket request IDs are used for responses; send idempotency_key (or
request ID) is the run correlation ID, never a new engine-generated ID.

Run buffers hold structured events with stable timestamps and sequence numbers.
SSE heartbeat comments remain a gateway transport concern. HTTP terminal replay
and live reattachment retain their existing behavior. WS uses its own outer
per-Bot delivery sequence across socket reconnects, while retaining the run event
sequence internally. A same-ID send retry acknowledges the original run without
replaying events already written locally.

Each run has immutable local Bot/session scope. WS event envelopes add the
original wire group/session scope from the accepted request. For V2, prefer an
explicit bcs_session_id, otherwise retain the complete bcs_group_id session form;
never key engine history on a truncated group session_key. Unqualified groups
use the BCS default session convention. Group Context text is already present
in V2 messages and is not rebuilt by Bridge.

## Configuration and durable state

Existing configuration continues to select gateway when mode is omitted.
Gateway still requires listen and bcs_to_provider_token. Upstream requires an
explicit ws/wss URL, and local-to-BCS Bot identity bindings; it does not require
an HTTP listener or HTTP token. Keep provider_id as the stable local database
namespace in both modes (it does not require Provider registration in upstream).
Keep provider_bot_ref as the local engine binding so an explicitly configured
mode switch can address existing session mappings. BCS itself must allow the
Bot's chosen delivery mode; Bridge does not modify BCS registration automatically.

Store reconnect credentials in the existing private SQLite database, scoped by
local namespace, server URL and local Bot reference. The supplied Bot identity
must match a restored credential identity and the handshake result. Persist
handshake credentials before accepting work; failed writes fail the connection.
Schema migration from the existing database is transactional. The default
database remains ~/.bcn-bridge/bridge-state.sqlite3. No engine transcript writes.

## WebSocket lifecycle and delivery semantics

One connection per configured Bot. States: disconnected -> connecting ->
bot.connect -> ready. Complete the handshake before draining application events.
Maintain request-response correlation for handshake and bot.status heartbeat.
Use a single ordered writer, explicit send errors, reconnect backoff, and bounded
network waits. Authentication/protocol/identity rejection is an explicit failure,
not an excuse to register a replacement Bot. Handle server kick as a stop rather
than fighting delivery-mode changes with reconnects.

Accepted runs belong to the Bridge process, not to one socket generation. Their
event subscription remains alive while the socket is disconnected. Delivery
cursors advance only after a successful local socket write; unsent terminal
events must remain retained even after engine completion. The queue must have
a bounded memory budget and overflow must terminate/report failure rather than
silently discard tool events or final. Process shutdown explicitly cancels runs;
normal run deadlines and user abort remain effective while disconnected.

V2 has no peer ACK or resume cursor: a write completed locally can still be lost
at a network boundary, and a failed write may have reached the peer. Do not claim
exactly-once or replay every previously written frame. In-process offline
buffering is supported; cross-process recovery of in-flight runs/events is not.
BCS ordinary group WS ingestion accepts the original run after Bot reconnect;
task/state-machine runs still obey their existing terminal/attempt guards.

Inject always commits before ACK, does not run an engine, and is merged into the
next send. Claim/complete/release semantics remain independent of network writes:
completion acknowledges engine consumption, not receipt of the WS final by BCS.

## Compatibility and verification

No BCS server, OpenClaw plugin, frontend, or protocol-version changes in this
feature. Reuse bcs-protocol V2 DTOs; do not depend on another concrete adapter.
Preserve JSON tool values and apply the existing top-level result unwrap once.
Unknown methods get explicit errors; do not claim chat.history/session.delete
support. Reject unsupported attachment-only input instead of accepting empty work.

Tests exercise real encoders, SQLite, Bridge runtime and loopback WebSockets with
deterministic engine subprocesses. Cover old gateway frames, config validation,
session isolation, ACK-before-event, heartbeat/reconnect, original run IDs,
offline completion then reconnect, no engine restart, inject across process
restart, unsupported V2 interaction, abort, shutdown, credentials persistence
failure, and queue overflow. No live model or real BCS messages are sent.

No global formatting. User-owned scripts and runtime credentials stay untracked.


## Running Bridge

Gateway configurations keep their existing mode default and startup command:

```sh
BRIDGE_CONFIG=/path/to/bridge-gateway.toml RUST_LOG=info cargo run -p bridge-provider
```

A minimal upstream configuration (replace the URL, Bot ID and working directory
with your own registered Bot values):

```toml
mode = "upstream"
provider_id = "local-bridge"
state_path = "~/.bcn-bridge/bridge-state.sqlite3"

[upstream]
url = "wss://bcs.example.com/ws/bot"
reconnect_interval_ms = 1000
heartbeat_interval_ms = 30000
connect_timeout_ms = 10000

[[bot]]
provider_bot_ref = "worker-1"
bot_id = "your-bot:your-owner"
engine = "cfuse-cc"
cwd = "/path/to/bot-workspace"
# cfuse_bin = "/path/to/cfuse"
# model = "your-engine-model"
# token = "initial-reconnect-token"
```

```sh
BRIDGE_CONFIG=/path/to/bridge-upstream.toml RUST_LOG=info cargo run -p bridge-provider
```

Add more `[[bot]]` entries to open independent connections. `provider_id` is a
local SQLite namespace in upstream mode, and `provider_bot_ref` selects the local
engine binding. Preserve both to reuse existing engine session mappings. A saved
identity must match the configured Bot ID; its latest returned token is reused
instead of the optional initial token. URL userinfo/query/fragment are rejected;
Bot credentials belong in the per-Bot token field and private database.

Version 2 of the local database adds a reconnect-identity table. Migration retains
all v1 mappings and inject receipts. The old v1 binary rejects a v2 database: use
a separate state path for rollback, or restore a backup made while Bridge was
stopped. Do not run two Bridge instances against the same database.

Each run retains at most 32 MiB of typed event data or 8192 events, plus a bounded
terminal overflow error. Each Bot admits at most 16 pending deliveries; further
new runs receive a retryable error until delivery drains. Terminal data is released
after successful local delivery, while a small idempotency record with a fixed-size,
length-delimited SHA-256 request fingerprint remains for five minutes after delivery. The Bot route cache admits at most 1024 recent routes and expires
completed routes after five minutes. These are local memory bounds, not durable
message queues. Every wire event is also checked against the 8 MiB frame limit by
its encoder; encoding failure ends the affected run with a bounded error.

Network reconnection does not regenerate coordination requests or engine turns.
A successful socket write is the local delivery boundary; V2 does not acknowledge
individual events. Therefore the narrow network-failure boundary can still lose
or duplicate a frame. Active runs and unsent frames do not survive Bridge process
restart. Completed engine mappings, committed inject messages and reconnect
credentials do survive restart. SQLite commit failures never produce a successful
inject ACK or a ready connection.

BCS must already permit the Bot to use WS delivery. A `bot.kicked` event stops that
Bot client; Bridge does not fight a delivery-mode switch by reconnecting. V2 does
not implement interaction authorization: when cfuse asks for authorization, the
run returns `unsupported_interaction` and ends. Existing gateway authorization
continues to work.
