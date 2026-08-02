# BCN New APIs Error Code Convention Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Apply the five-digit BCN response-code convention and explicit error-condition mapping to all 52 operations under `new_apis`.

**Architecture:** Operations declare concise `x-bcn-error-codes` metadata. The existing Python bundler validates the metadata and materializes strict success/error response schemas and examples for Swagger, while `_shared.yaml` remains the common Envelope source.

**Tech Stack:** OpenAPI 3.1 YAML, Python 3, PyYAML.

---

### Task 1: Add failing convention checks

**Files:**
- Test: inline Python verification against `src/bcs-internal/docs/new_apis`

**Steps:**

1. Assert `Envelope.code.example == 20000` and descriptions specify five digits.
2. Assert every non-2xx operation response has a matching `x-bcn-error-codes` entry.
3. Run the assertion and confirm it fails on the existing six-digit examples and absent metadata.

### Task 2: Implement shared validation and Swagger materialization

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/_shared.yaml`
- Modify: `src/bcs-internal/docs/new_apis/serve_api_docs.py`
- Modify: `src/bcs-internal/docs/new_apis/README.md`

**Steps:**

1. Change Envelope examples and descriptions to the five-digit convention.
2. Validate the shape of every `x-bcn-error-codes` map, five-digit code, HTTP prefix, non-empty message/condition, response status coverage, and global code-message uniqueness.
3. Resolve shared response refs in generated bundles and add response-specific `const` schemas and examples.
4. Constrain 200/201/202 response codes to 20000/20100/20200.
5. Add the convention and authoring syntax to the README.

### Task 3: Declare Actor/Bot error mappings

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/openapi/actors-bots.yaml`

**Steps:**

1. Add concrete mappings for parameter validation, authentication, acting-Actor authorization, ownership, visibility and resource lookup.
2. Verify `GET /actors` produces distinct examples including 40301.

### Task 4: Declare all remaining OpenAPI error mappings

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/openapi/bot-registration.yaml`
- Modify: `src/bcs-internal/docs/new_apis/openapi/friendships.yaml`
- Modify: `src/bcs-internal/docs/new_apis/openapi/groups.yaml`
- Modify: `src/bcs-internal/docs/new_apis/openapi/invitations.yaml`
- Modify: `src/bcs-internal/docs/new_apis/openapi/providers.yaml`
- Modify: `src/bcs-internal/docs/new_apis/openapi/sessions.yaml`

**Steps:**

1. Add an error-code map to every operation with a non-2xx response.
2. Reuse globally stable code-message pairs for authentication, lookup and common validation.
3. Use domain-specific codes for friendship, Group, Session, invitation, Provider and registration conflicts.

### Task 5: Declare Internal API mappings and verify all bundles

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/internalapi/providers.yaml`
- Modify: `src/bcs-internal/docs/new_apis/internalapi/state-machine-runs.yaml`

**Steps:**

1. Add concrete internal-service authentication, Provider and state-machine error mappings.
2. Run the initial convention assertion and require it to pass for all 52 operations.
3. Run `python3 src/bcs-internal/docs/new_apis/serve_api_docs.py --check`.
4. Inspect generated `/actors` 403 schema and examples for code 40301.
5. Confirm no Rust runtime or historical API files were modified.
