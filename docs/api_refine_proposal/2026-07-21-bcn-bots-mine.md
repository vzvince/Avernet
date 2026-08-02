# BCN Bots Mine API Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the actor-scoped Bot ownership query with a Human-relative `bots/mine` endpoint shown only in the Bots category.

**Architecture:** Make a contract-only change in the target OpenAPI fragment. Preserve filtering, pagination, response schemas, authentication, and operation count while removing the redundant actor path parameter and actor-mismatch error.

**Tech Stack:** OpenAPI 3.1 YAML, Python 3, PyYAML, Swagger UI aggregation script.

---

### Task 1: Replace the actor-scoped path

**Files:**
- Modify: `src/bcs-internal/docs/new_apis/openapi/actors-bots.yaml:200-245`

**Step 1: Run the failing contract assertion**

Load `actors-bots.yaml` and assert that:

- `/openapi/v1/bcn/bots/mine` exists;
- `/openapi/v1/bcn/actors/{actor_id}/bots` does not exist;
- the operation ID is `listMyBots`;
- the only tag is `bots`;
- the endpoint has no path parameter and no `404` response.

**Step 2: Verify the assertion fails**

Expected: failure because the current path remains actor-scoped and uses both `actors` and `bots` tags.

**Step 3: Apply the minimal OpenAPI change**

Change the path and operation metadata, remove `ActorIdPath`, remove the actor mismatch error, and update summary/description text. Keep HumanCookie-only security, filters, paging, and `ActorBotPageResponse` unchanged.

**Step 4: Run focused and aggregate validation**

Run the contract assertion again, then run:

```bash
python3 src/bcs-internal/docs/new_apis/serve_api_docs.py --check
git diff --check
```

Expected: all Swagger bundles build; OpenAPI remains 35 paths and 46 operations; Internal API remains 6 paths and 6 operations.

**Step 5: Commit the implementation**

Stage the modified OpenAPI fragment and this plan, then commit with a documentation-focused message.
