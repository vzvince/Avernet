# Current Actor Groups Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `GET /groups/my` for authenticated human and bot callers, then migrate `bcs-cli list-groups` so it no longer needs a locally known bot UUID.

**Architecture:** Keep caller resolution and HTTP serialization in `bcs-http`, reusing the existing group-route actor resolver and `GroupQueryService::list_bot_groups`. Preserve `GET /bots/{id}/groups` as a compatibility route by extracting its current query/merge behavior into a shared adapter helper. The CLI authenticates to the new route, obtains `actor_id` from the response, and scopes pagination continuation state to that server-resolved actor.

**Tech Stack:** Rust, Axum 0.8, Serde, Reqwest, Clap, Tower contract tests, Wiremock CLI E2E tests.

---

### Task 1: Add the authenticated current-actor HTTP route

**Files:**
- Modify: `src/bcs/crates/adapters/http/bcs-http/tests/groups_contract.rs`
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/router.rs:178-182`
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/routes/groups.rs:520-639`

**Step 1: Write failing route contract tests**

Add a bot-principal helper next to `static_auth_chain`:

```rust
fn static_bot_auth_chain(bot_uuid: &str) -> Arc<AuthPluginChain> {
    let principal = AuthPrincipal {
        bot_uuid: Some(bot_uuid.to_string()),
        ..Default::default()
    };
    Arc::new(AuthPluginChain::new(vec![Box::new(
        StaticAuthPlugin::with_principal(principal),
    )]))
}
```

Add three focused tests using `RecordingGroupQuery`:

```rust
#[tokio::test]
async fn my_groups_resolves_bot_principal_and_preserves_query() {
    // Build Services with RecordingGroupQuery and HttpAppState.with_auth_chain(...).
    // GET /groups/my?group_kind=normal&offset=4&limit=5&q=05&include_session_groups=false
    // Assert 200, response actor_id == "bot-current", and no bot_uuid field.
    // Assert BotGroupListCommand.bot_id == "bot-current" and filters are preserved.
}

#[tokio::test]
async fn my_groups_resolves_human_identity() {
    // Build state with ChainUserIdentityPort(static_auth_chain("alice", "Alice")).
    // GET /groups/my?include_session_groups=false
    // Assert 200 and actor_id == "human_alice".
    // Assert the recorded BotGroupListCommand uses "human_alice".
}

#[tokio::test]
async fn my_groups_rejects_anonymous_caller_without_shadowing_group_detail() {
    // Assert anonymous GET /groups/my returns 401.
    // Assert GET /groups/group-1 still returns the existing group detail response.
    // Existing explicit GET /bots/driver-bot/groups remains covered unchanged.
}
```

Use `include_session_groups=false` in identity-focused tests so the noop session service does not obscure the asserted command.

**Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-http --test groups_contract my_groups -- --nocapture
```

Expected: FAIL because `/groups/my` is handled as `GET /groups/{id}` and does not return the current-actor list contract.

**Step 3: Mount the static route**

Add the route beside the group collection/detail routes:

```rust
.route("/groups/my", get(routes::groups::list_my_groups))
```

Keep `GET /groups/{id}` and `GET /bots/{id}/groups` mounted. Do not depend on declaration order for correctness; the contract test proves coexistence.

**Step 4: Extract the shared actor-group list helper**

In `routes/groups.rs`, introduce an adapter-only page type:

```rust
struct ActorGroupListPage {
    items: Vec<Value>,
    total: u64,
    offset: u64,
    limit: u64,
}
```

Move the existing `list_bot_groups` query, optional session-group union, filtering, ordering, and pagination into:

```rust
async fn list_actor_groups(
    state: &HttpAppState,
    actor_id: &str,
    query: ListBotGroupsQuery,
) -> Result<ActorGroupListPage, HttpAdapterError>
```

Replace every use of the old local `bot_uuid` inside that logic with `actor_id`. Return the page instead of serializing identity fields inside the helper.

Keep the legacy wrapper response unchanged:

```rust
pub async fn list_bot_groups(
    State(state): State<HttpAppState>,
    Path(bot_uuid): Path<String>,
    Query(query): Query<ListBotGroupsQuery>,
) -> Result<Json<Value>, HttpAdapterError> {
    let page = list_actor_groups(&state, &bot_uuid, query).await?;
    Ok(Json(serde_json::json!({
        "bot_uuid": bot_uuid,
        "items": page.items,
        "total": page.total,
        "offset": page.offset,
        "limit": page.limit,
    })))
}
```

**Step 5: Implement the new handler**

Use the existing bot-first, human-fallback resolver in the same module:

```rust
pub async fn list_my_groups(
    State(state): State<HttpAppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<ListBotGroupsQuery>,
) -> Result<Json<Value>, HttpAdapterError> {
    let actor_id = resolve_actor_caller(&state, &headers, &uri).await?;
    let page = list_actor_groups(&state, &actor_id, query).await?;
    Ok(Json(serde_json::json!({
        "actor_id": actor_id,
        "items": page.items,
        "total": page.total,
        "offset": page.offset,
        "limit": page.limit,
    })))
}
```

Do not add actor IDs to query parameters or request bodies. Do not change the Service API command in this task.

**Step 6: Run HTTP tests**

Run:

```bash
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-http --test groups_contract my_groups -- --nocapture
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-http --test groups_contract
```

Expected: PASS, including existing explicit-actor and group-detail assertions.

**Step 7: Commit**

```bash
git add src/bcs/crates/adapters/http/bcs-http/src/router.rs \
  src/bcs/crates/adapters/http/bcs-http/src/routes/groups.rs \
  src/bcs/crates/adapters/http/bcs-http/tests/groups_contract.rs
git commit -m "feat(bcs-http): add current actor groups route"
```

### Task 2: Migrate `bcs-cli list-groups` to the authenticated route

**Files:**
- Modify: `src/bcs/crates/tools/bcs-cli/tests/e2e/groups_test.rs`
- Modify: `src/bcs/crates/tools/bcs-cli/src/client.rs:31-37,1681-1715`
- Modify: `src/bcs/crates/tools/bcs-cli/src/main.rs:66-110,2741-2842,4014-4026`

**Step 1: Change the primary CLI E2E test to omit the local bot UUID**

Rewrite the session file in `list_groups_uses_current_bot_and_excludes_session_only_groups` with only the token and BCS URL:

```rust
std::fs::write(
    ctx.session_path(),
    serde_json::to_vec(&serde_json::json!({
        "token": ctx.session.token,
        "bcs_url": ctx.session.bcs_url,
    }))
    .unwrap(),
)
.unwrap();
```

Expect `GET /groups/my`, Bearer authentication, and this envelope:

```json
{
  "actor_id": "bot-current",
  "items": [],
  "total": 0,
  "offset": 0,
  "limit": 20
}
```

Update the remaining group-list mocks from `/bots/{uuid}/groups` to `/groups/my` and include `actor_id` in every valid page response. Keep the malformed-envelope test without the required field.

**Step 2: Add continuation compatibility tests**

Update the cross-identity test to have the second `/groups/my` response return a different `actor_id`, then assert the CLI rejects it as belonging to another actor.

Add a unit test that decodes a version-1 continuation JSON containing `bot_uuid` and maps it into actor-scoped state. Keep the unsupported-version test.

**Step 3: Run the CLI tests and verify they fail**

Run:

```bash
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-cli --test e2e groups_test -- --nocapture
```

Expected: FAIL because the client still requires the session UUID and calls `/bots/{uuid}/groups`.

**Step 4: Add actor identity to the page contract and new client method**

Change the client page:

```rust
#[derive(Debug, Deserialize)]
pub struct BotGroupListPage {
    pub actor_id: String,
    pub items: Vec<serde_json::Value>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}
```

Add a self-query method without an actor parameter:

```rust
pub async fn list_my_groups(
    &self,
    offset: u64,
    limit: u64,
    include_session_groups: bool,
) -> Result<BotGroupListPage> {
    let url = format!("{}/groups/my", self.base_url);
    let response = self
        .add_auth(self.http_client.get(&url).query(&[
            ("offset", offset.to_string()),
            ("limit", limit.to_string()),
            ("include_session_groups", include_session_groups.to_string()),
        ]))
        .send()
        .await
        .context("Failed to list current actor groups")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("List current actor groups failed ({}): {}", status, body));
    }
    response.json().await.context("Invalid current actor groups response")
}
```

Retain `list_bot_groups` for existing library consumers and its legacy response shape. If both methods share request/error handling, extract only a small private helper; do not change its public signature.

**Step 5: Evolve continuation state compatibly**

Emit version 2 with actor terminology while accepting version 1:

```rust
const GROUP_CONTINUATION_VERSION: u8 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct GroupContinuation {
    version: u8,
    #[serde(default, alias = "bot_uuid")]
    actor_id: Option<String>,
    next_offset: u64,
    batch_size: u64,
}
```

`decode_group_continuation` must accept versions 1 and 2, require a non-empty resolved actor ID for both, and continue rejecting every other version. New tokens serialize `actor_id` only.

**Step 6: Remove the pre-request UUID dependency from the command**

In the `Commands::ListGroups` arm:

- remove `resolve_my_bot_uuid()`;
- call `client.list_my_groups(offset, batch_size, false)`;
- take the actor identity from the first page;
- require every additional page to return the same actor identity;
- when `--continue` is present, compare its actor identity with the response identity before accumulating items;
- encode new continuation state with the response `actor_id`;
- print `Groups for current actor {actor_id}` in human output.

The credential, not the continuation token, remains the authorization source.

**Step 7: Run focused CLI tests**

Run:

```bash
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-cli --test e2e groups_test -- --nocapture
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-cli group_continuation -- --nocapture
```

Expected: PASS. The no-UUID session test proves the original defect is removed.

**Step 8: Commit**

```bash
git add src/bcs/crates/tools/bcs-cli/src/client.rs \
  src/bcs/crates/tools/bcs-cli/src/main.rs \
  src/bcs/crates/tools/bcs-cli/tests/e2e/groups_test.rs
git commit -m "feat(bcs-cli): list groups from authenticated actor"
```

### Task 3: Update the user-facing contract documentation and verify the change

**Files:**
- Modify: `src/bcs/CLAUDE.md:370-385`
- Modify: `src/bcs/crates/tools/bcs-cli/bcs-coordination/references/group.md:7-14,173-205`

**Step 1: Update the BCS endpoint inventory**

Add:

```markdown
| `/groups/my` | GET | List groups for the authenticated human or bot actor |
```

Keep `/groups` and `/bots/{id}/groups` documented with their distinct all-groups and explicit-actor meanings.

**Step 2: Correct the CLI reference**

Replace the stale `list-groups [--mine]` documentation with the current command:

```markdown
| `list-groups` | `[--batch-size]`, `[--continue]`, `[--all]` | 列出当前认证 Human 或 Bot 正式参与的群组 |
```

Document that identity comes from server-side authentication and that the CLI does not require `session.json.bot_uuid`. Remove the obsolete `--mine` examples.

**Step 3: Run complete affected-crate verification**

Run:

```bash
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-http --test groups_contract
cargo test --manifest-path src/bcs/Cargo.toml --package bcs-cli
cargo check --manifest-path src/bcs/Cargo.toml --package bcs-http --all-targets
cargo check --manifest-path src/bcs/Cargo.toml --package bcs-cli --all-targets
```

Expected: all commands exit 0. Do not run `cargo fmt` or any global formatter.

**Step 4: Inspect the final diff**

Run:

```bash
git diff --check
git status --short
git diff --stat HEAD~2
```

Expected: no whitespace errors; only the documented HTTP adapter, CLI, tests, and docs are changed. Existing unrelated untracked files remain untouched.

**Step 5: Commit**

```bash
git add src/bcs/CLAUDE.md \
  src/bcs/crates/tools/bcs-cli/bcs-coordination/references/group.md
git commit -m "docs(bcs): document current actor group listing"
```
