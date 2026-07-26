# Organization Member Pagination Design

## Goal

Add stable, offset-based pagination to the provider-scoped Organization member
listing API without changing Organization authorization or runtime discovery.

## API contract

`GET /providers/{provider_id}/organizations/{organization_code}/members`
accepts optional `include_disabled`, `role`, `offset`, and `limit` query
parameters. `offset` defaults to `0`; `limit` defaults to `50` and is capped
at `200`. Results are sorted by `bot_uuid ASC` and return `members`, `offset`,
`limit`, and `total`.

## Architecture

The HTTP adapter parses and validates pagination, then calls the existing
Organization application service. Pagination data travels through the
application/core/repository contracts. Memory, SQLite, and MySQL repository
implementations own filtering, deterministic ordering, page selection, and
the matching count query. No delivery adapter reaches a store directly.

## SQL and index decision

The current `idx_member_org_disabled_role` index filters the common scoped
query but does not cover the required `bot_uuid` ordering. Add (do not remove)
`idx_member_org_disabled_role_bot (env, organization_code, disabled, role,
bot_uuid)`. The unique key already supports the no-role ordering path only
partially because it does not include `disabled`; retain it for integrity.

Offset pagination has linear skip cost at very deep offsets. The expected
Organization size is hundreds of members, so this is acceptable for V1.
Cursor pagination can replace it if member counts grow substantially.

## Tests

Test the wire route forwards pagination and returns metadata; test defaults,
bounds, and invalid values; test repository filtering, deterministic order,
page boundaries, and matching totals for memory and DB-backed stores. Verify
the generated SQL uses parameterized `LIMIT`/`OFFSET`, a count query, and the
new MySQL migration index.
