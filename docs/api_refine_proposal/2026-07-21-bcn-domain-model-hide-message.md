# Hide Message From BCN Domain Models View Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove `Message` from the standalone BCN Domain Models Swagger page without removing its authoritative Schema or breaking Session message APIs.

**Architecture:** Change only the presentation allowlist used by `build_domain_models()`. The complete `domain-models.yaml` and all API bundles continue to include `Message`.

**Tech Stack:** Python 3, PyYAML, OpenAPI 3.1, Swagger UI 5.

---

### Task 1: Remove Message from the presentation projection

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/serve_api_docs.py:27`
- Modify: `src/bcs-internal/docs/new_apis/README.md:43`

**Step 1: Run the failing presentation assertion**

Assert that `build_domain_models()` exposes the approved ten ordered schemas and does not expose `Message`.

**Step 2: Verify the assertion fails**

Expected failure: the current projection contains eleven schemas including `Message`.

**Step 3: Apply the minimal change**

Delete `Message` from `DOMAIN_MODEL_VIEW_SCHEMAS` and state in the README that Message remains an API-supporting Schema but is not shown as a standalone domain object.

**Step 4: Verify the projection and full API bundles**

Assert:

- Domain Models exposes exactly ten ordered top-level schemas.
- `Message` remains in `domain-models.yaml`.
- OpenAPI and All API bundles still contain `Message`.
- Session message request/response refs remain valid.

Run `python3 src/bcs-internal/docs/new_apis/serve_api_docs.py --check`; expect `domain-models ... schemas=10` and all bundle validation to pass.
