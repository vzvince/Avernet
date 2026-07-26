# BCN Bots Mine API Design

## Decision

Replace `GET /openapi/v1/bcn/actors/{actor_id}/bots` with
`GET /openapi/v1/bcn/bots/mine`.

The endpoint is listed only under the `bots` category. It accepts HumanCookie
authentication only and always resolves `mine` from the current logged-in Human;
Bot Runtime and AgentPass identities cannot call it.

## Semantics

- V1 returns BotActors created by the current Human and reports `creates` in each
  relation projection.
- The path remains valid when the relation later expands from `creates` to
  `creates + owns`.
- Existing text, collaboration status, reachability, offset, and limit filters
  remain unchanged.
- Removing `actor_id` eliminates a redundant caller-supplied identity and the
  corresponding actor-mismatch `404`; missing or invalid HumanCookie returns
  `401`.

## Scope

Update only the target `new_apis` OpenAPI contract and its dependent response
description. Do not change current Rust routes or the historical API catalog.
