# Organization Member Pagination Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add stable offset pagination and totals to provider-scoped Organization member listings.

**Architecture:** The HTTP adapter validates `offset` and `limit`, application and core services forward a page query, and the Organization repository returns an ordered page plus count. Store implementations own SQL and in-memory paging; the MySQL migration adds an index that covers the role-filtered ordered page.

**Tech Stack:** Rust, Axum, async traits, SQLite/MySQL DB plugin, Cargo tests.

---

### Task 1: Define paged member-list contracts

**Files:**
- Modify: `src/bcs/crates/contracts/bcs-protocol/src/http/organizations.rs`
- Modify: `src/bcs/crates/service-api/bcs-service-api/src/application/organization.rs`
- Modify: `src/bcs/crates/service-api/bcs-service-api/src/core/organization.rs`
- Modify: `src/bcs/crates/service-api/bcs-service-api/src/port/repo/organization.rs`
- Modify: re-export modules that expose changed types.

**Step 1: Write failing contract tests**

Add route and service test expectations for `offset`, `limit`, and `total`.

**Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p bcs-http --test organizations_contract pagination`

Expected: FAIL because pagination is not accepted or returned.

**Step 3: Add minimal contract types**

Define page query/result types and extend the member-list response without adding unrelated cursor support.

**Step 4: Re-run the targeted test**

Run: `cargo test -p bcs-http --test organizations_contract pagination`

Expected: PASS after downstream implementation tasks.

### Task 2: Implement repository paging and count

**Files:**
- Modify: `src/bcs/crates/services/bcs-organization-store/src/memory.rs`
- Modify: `src/bcs/crates/services/bcs-organization-store/src/lib.rs`
- Modify: `src/bcs/crates/test-support/bcs-test-support/src/noop.rs`
- Modify: `src/bcs/crates/test-support/bcs-test-support/src/contract/repo/mod.rs`
- Test: `src/bcs/crates/services/bcs-organization-store/tests/conformance_organization_repo.rs`

**Step 1: Write failing repository tests**

Create members in non-sorted order; assert role/disabled filters, bot UUID ordering, page selection, and total count.

**Step 2: Run and verify RED**

Run: `cargo test -p bcs-organization-store --test conformance_organization_repo pagination`

Expected: FAIL because the repository returns an unpaged vector.

**Step 3: Implement minimal repository behavior**

Sort memory results by `bot_uuid`, select the requested range, and compute total before selection. In DB stores issue parameterized ordered `LIMIT/OFFSET` and matching `COUNT(*)` statements.

**Step 4: Run and verify GREEN**

Run: `cargo test -p bcs-organization-store --test conformance_organization_repo pagination`

Expected: PASS.

### Task 3: Wire application/core and HTTP adapter

**Files:**
- Modify: `src/bcs/crates/services/bcs-organization/src/application.rs`
- Modify: `src/bcs/crates/services/bcs-organization/src/core.rs`
- Modify: `src/bcs/crates/adapters/http/bcs-http/src/routes/organizations.rs`
- Test: `src/bcs/crates/services/bcs-organization/tests/management.rs`
- Test: `src/bcs/crates/adapters/http/bcs-http/tests/organizations_contract.rs`

**Step 1: Write failing route tests**

Assert explicit query values reach the application service, defaults are returned in the response, and invalid/over-limit values receive 400.

**Step 2: Run and verify RED**

Run: `cargo test -p bcs-http --test organizations_contract pagination`

Expected: FAIL because query values are ignored and metadata is absent.

**Step 3: Implement minimal validation/wiring**

Use defaults `offset=0`, `limit=50`; reject `limit=0` and values above `200`; preserve existing authorization and role behavior.

**Step 4: Run and verify GREEN**

Run: `cargo test -p bcs-http --test organizations_contract pagination`

Expected: PASS.

### Task 4: Add the MySQL index and verify all affected suites

**Files:**
- Modify: `src/bcs/migrations/mysql/003_add_organizations.sql`
- Test: store SQL-recording tests and migration assertions where applicable.

**Step 1: Write a failing SQL assertion**

Assert the generated page SQL uses deterministic order and bound pagination values.

**Step 2: Run and verify RED**

Run: `cargo test -p bcs-organization-store`

Expected: FAIL until paged SQL is implemented.

**Step 3: Add index only**

Add `idx_member_org_disabled_role_bot` without removing existing indexes.

**Step 4: Run verification**

Run: `cargo test -p bcs-organization -p bcs-organization-store -p bcs-http`

Expected: PASS.
