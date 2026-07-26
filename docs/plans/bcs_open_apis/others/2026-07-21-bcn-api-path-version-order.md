# BCN New APIs Path Version Order Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move the version segment before the BCN namespace for every OpenAPI and Internal API path under `new_apis`.

**Architecture:** Perform a scope-limited mechanical rewrite of path strings and update the Python fragment validator. Preserve operation IDs, request/response schemas, authentication, and error-code metadata unchanged.

**Tech Stack:** OpenAPI 3.1 YAML, Markdown, Python 3, PyYAML.

---

### Task 1: Rewrite and validate all target paths

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/openapi/*.yaml`
- Modify: `src/bcs-internal/docs/new_apis/internalapi/*.yaml`
- Modify: `src/bcs-internal/docs/new_apis/serve_api_docs.py`
- Modify: `src/bcs-internal/docs/new_apis/README.md`

**Step 1: Run the failing prefix assertion**

Assert all OpenAPI paths begin with `/openapi/v1/bcn/`, all Internal API paths begin with `/api/v1/bcn/`, and neither old prefix occurs under `new_apis`.

**Step 2: Verify the assertion fails**

Expected: all 41 paths still use the old `bcn/v1` order.

**Step 3: Apply the mechanical rewrite**

- Replace `/openapi/bcn/v1/` with `/openapi/v1/bcn/` in `new_apis`.
- Replace `/api/bcn/v1/` with `/api/v1/bcn/` in `new_apis`.
- Update `validate_fragments()` expected prefixes.

**Step 4: Run full verification**

Assert 35 OpenAPI paths and 6 Internal API paths use the new prefixes, old prefixes have zero matches, and operation counts remain 46 and 6. Run `serve_api_docs.py --check` and `git diff --check`.
