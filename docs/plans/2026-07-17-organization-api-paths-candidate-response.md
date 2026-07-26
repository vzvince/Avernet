# Organization API Paths and Candidate Response Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move organization detail GET/PATCH to provider-independent paths and replace candidate Bot capabilities with a nullable name in the HTTP response.

**Architecture:** Keep provider-admin authentication in the application layer. The new organization detail handlers pass the existing token-only `OrganizationMemberAuth`; the application resolves the provider ID and delegates to the existing manager-authorized core methods. Keep full capabilities in the internal candidate model for filtering, and narrow only the HTTP DTO.

**Tech Stack:** Rust, Axum, Serde, async-trait, Tokio, Cargo integration tests.

---

### Task 1: Pin the breaking HTTP contract

**Files:**
- Modify: `src/bcs/crates/adapters/http/bcs-http/tests/organizations_contract.rs`

**Step 1: Write failing route tests**

Change the successful organization detail requests to:

```rust
("GET", "/organizations/promo-2026", None, StatusCode::OK),
("PATCH", "/organizations/promo-2026", Some(json!({
    "name":"Promo 2026 updated",
    "description":null,
    "disabled":false
})), StatusCode::OK),
```

Add a test that sends GET and PATCH to
`/providers/provider-a/organizations/promo-2026` and expects `404 NOT_FOUND`.

Update the recording candidate fixture to set
`capabilities.name = Some("Bot B".to_string())`, then add a response assertion:

```rust
assert_eq!(json["bots"][0], json!({
    "bot_uuid": "bot-b",
    "provider_id": "provider-b",
    "name": "Bot B"
}));
assert!(json["bots"][0].get("capabilities").is_none());
```

Add a second fixture mode or direct serialization assertion showing an absent
capability name becomes JSON `null`.

**Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p bcs-http --test organizations_contract provider_scoped_organization_routes_call_application_service
cargo test -p bcs-http --test organizations_contract provider_prefixed_organization_detail_routes_are_removed
cargo test -p bcs-http --test organizations_contract candidate_bots_response_exposes_name_without_capabilities
```

Expected: route tests fail because the new path is not registered and the old
path still exists; the candidate response test fails because it still returns
`capabilities` and has no top-level `name`.

### Task 2: Move organization GET/PATCH and reuse token-only auth

**Files:**
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/router.rs`
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/routes/organizations.rs`
- Modify: `src/bcs/crates/service-api/bcs-service-api/src/application/organization.rs`
- Modify: `src/bcs/crates/services/bcs-organization/src/application.rs`
- Modify: `src/bcs/crates/adapters/http/bcs-http/tests/organizations_contract.rs`
- Modify: `src/bcs/crates/services/bcs-organization/tests/management.rs`
- Modify: `src/bcs/crates/test-support/bcs-test-support/src/noop.rs`
- Modify: `src/bcs/scripts/e2e-test/stories.sh`

**Step 1: Change the Service API auth inputs**

Use the existing token-only type for update and get:

```rust
pub struct UpdateOrganizationCommand {
    pub auth: OrganizationMemberAuth,
    // existing fields unchanged
}

async fn get(
    &self,
    auth: OrganizationMemberAuth,
    code: &str,
) -> ServiceResult<Organization>;
```

Update the real, recording, and no-op implementations to match. In
`OrganizationManagement`, resolve the provider ID through the existing
`authenticate_member` helper before calling `get_for_manager` or
`update_for_manager`.

**Step 2: Change the handlers and router**

Register:

```rust
.route(
    "/organizations/{organization_code}",
    get(routes::organizations::get_organization)
        .patch(routes::organizations::patch_organization),
)
```

Remove the provider-prefixed organization detail route. Change both handlers to
extract `Path(organization_code): Path<String>` and create auth with
`organization_member_auth(&headers)`.

**Step 3: Update callers and E2E paths**

Use `member_auth(&provider)` in organization service tests that construct
`UpdateOrganizationCommand`. Change the organization read and update requests
in `src/bcs/scripts/e2e-test/stories.sh` to `/organizations/${organization_code}`.

**Step 4: Run route and service tests to verify GREEN**

Run:

```bash
cargo test -p bcs-http --test organizations_contract
cargo test -p bcs-organization --test management
```

Expected: all tests pass, including new paths and old-path 404 assertions.

### Task 3: Narrow the candidate Bot wire DTO

**Files:**
- Modify: `src/bcs/crates/contracts/bcs-protocol/src/http/organizations.rs`
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/routes/organizations.rs`
- Test: `src/bcs/crates/adapters/http/bcs-http/tests/organizations_contract.rs`

**Step 1: Replace capabilities with nullable name**

Define the wire response as:

```rust
pub struct OrganizationCandidateBotResponse {
    pub bot_uuid: String,
    pub provider_id: String,
    pub name: Option<String>,
}
```

In `candidate_to_response`, move the name out of the internal capabilities:

```rust
let name = bot.capabilities.name;
OrganizationCandidateBotResponse {
    bot_uuid: bot.bot_uuid,
    provider_id: bot.provider_id,
    name,
}
```

Do not change `OrganizationCandidateBot` or candidate filtering in the core.

**Step 2: Run candidate contract tests to verify GREEN**

Run:

```bash
cargo test -p bcs-http --test organizations_contract candidate_bots
```

Expected: candidate response tests pass with `name`, without `capabilities`, and
with existing pagination metadata.

### Task 4: Format and verify the affected BCS workspace

**Files:**
- Verify all modified Rust and shell files above.

**Step 1: Format Rust**

Run:

```bash
cargo fmt --all -- --check
```

If the check reports formatting differences, run `cargo fmt --all`, inspect the
diff, and repeat the check.

**Step 2: Run focused verification**

Run:

```bash
cargo test -p bcs-http --test organizations_contract
cargo test -p bcs-organization --test management
cargo check -p bcs-http -p bcs-organization -p bcs-service-api -p bcs-protocol
```

Expected: every command exits successfully with no failing tests or compile
errors.

**Step 3: Inspect the final diff**

Run:

```bash
git diff --check
git status --short
```

Confirm only the design/plan and requested implementation files changed; leave
pre-existing untracked files untouched.
