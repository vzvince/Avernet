# BCN RoutingPolicy Sender Routes Removal Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove `sender_routes` from the new BCN `RoutingPolicy` domain model while retaining current runtime and database compatibility.

**Architecture:** Treat `src/bcs-internal/docs/new_apis/domain-models.yaml` as the authoritative target wire schema and keep `src/bcs-internal/docs/bcn-domain-model.md` aligned as the high-level domain description. Do not modify legacy API documents, Rust domain code, routing execution, or database migrations.

**Tech Stack:** OpenAPI 3.1 YAML, Markdown, Python/PyYAML validation script.

---

### Task 1: Remove `sender_routes` from the target domain model

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/domain-models.yaml:329`
- Modify: `src/bcs-internal/docs/bcn-domain-model.md:58`

**Step 1: Run the failing schema assertion**

Load `domain-models.yaml` and the generated Domain Models projection, then assert that `RoutingPolicy.required` and `RoutingPolicy.properties` do not contain `sender_routes`.

**Step 2: Verify the assertion fails**

Run the inline Python assertion. Expected: failure because the current schema contains `sender_routes`.

**Step 3: Apply the minimal documentation change**

- Change `RoutingPolicy.required` to `[mode, default_delivery]`.
- Delete the `sender_routes` property.
- Describe `default_delivery` as the behavior used when there is no explicit routing target.
- Delete `senderRoutes` from the high-level Group structure and field table.

**Step 4: Verify the assertion and all bundles pass**

Run:

```bash
python3 src/bcs-internal/docs/new_apis/serve_api_docs.py --check
```

Expected: All, OpenAPI, Internal API, and Domain Models bundles build successfully; the generated `RoutingPolicy` contains only `mode` and `default_delivery`.

**Step 5: Review scope**

Confirm no files under `ocb-public/src/bcs`, no database migration, and no historical API catalog were modified.
