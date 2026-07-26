# BCS Discover Excludes Requester Design

**Date:** 2026-07-23

## Goal

When an authenticated bot calls `GET /bots/discover`, exclude the bot identified
by the caller's token from the discovery results. Apply the rule to both normal
and organization-scoped discovery. Human and service callers do not have a
requester bot to exclude and retain their current behavior.

## Root Cause

The HTTP adapter already resolves the authenticated bot and passes its UUID as
`BotDiscoveryCommand::requester_bot_id`. The application service currently uses
that field only to authorize organization-scoped discovery; neither discovery
path removes the requester from its candidates.

## Design

Make requester exclusion an application-layer discovery invariant:

- Document `requester_bot_id` as the authenticated requester that is excluded
  from returned results when present.
- In normal discovery, discard a candidate whose UUID equals
  `requester_bot_id` before converting candidates into response entries.
- In organization-scoped discovery, skip the requester before applying
  selector, visibility, friendship, and response mapping logic.
- Continue deriving `count` from the final filtered entries.

This keeps the behavior consistent for the CLI, HTTP consumers, and future
delivery adapters without moving domain policy into transport or presentation
code.

## Testing

Use application-service tests to prove:

- normal discovery excludes the authenticated requester while retaining other
  matching bots;
- organization-scoped discovery excludes the authenticated requester while
  retaining other eligible organization members;
- discovery without `requester_bot_id` retains existing behavior.

Run the focused `bcs-bot` test target, then the relevant crate test suite.
