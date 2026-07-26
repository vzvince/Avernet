# Organization API Path and Candidate Response Design

## Scope

Make two breaking HTTP contract changes for organization management:

- Replace `GET` and `PATCH`
  `/providers/{provider_id}/organizations/{organization_code}` with `GET` and
  `PATCH` `/organizations/{organization_code}`.
- Remove `capabilities` from organization candidate Bot responses and add the
  optional `name` field instead.

The old provider-prefixed detail routes are removed rather than retained as
compatibility aliases. Provider-prefixed organization creation and listing
remain unchanged.

## Authentication and authorization

The new organization detail routes extract the existing provider admin Bearer
Token without a provider ID from the path. They reuse the token-only
`OrganizationMemberAuth` flow, which authenticates the provider admin Token,
rejects disabled providers, resolves the provider ID, and then delegates to the
existing manager authorization in the organization core service.

No new authentication type is introduced. `UpdateOrganizationCommand` uses the
existing token-only auth value because the provider ID is no longer a route
input.

## Candidate Bot response

The internal `OrganizationCandidateBot` continues to carry full Bot
capabilities because candidate filtering depends on them. Only the HTTP wire
projection changes. Each response item contains:

```json
{
  "bot_uuid": "bot-b",
  "provider_id": "provider-b",
  "name": "Bot B"
}
```

`name` is nullable and serializes as `null` when the capability name is absent.
The surrounding `bots`, `offset`, `limit`, and `total` page shape is unchanged.

## Error handling

Existing authentication, authorization, validation, not-found, and persistence
error mappings remain unchanged. Requests to the removed provider-prefixed GET
and PATCH routes return HTTP 404.

## Tests

HTTP contract tests will first assert:

- the new GET and PATCH routes call the application service successfully;
- the old GET and PATCH routes return HTTP 404;
- candidate Bot items contain `bot_uuid`, `provider_id`, and nullable `name`;
- candidate Bot items do not contain `capabilities`;
- existing pagination metadata remains unchanged.

After observing the contract tests fail for the expected missing behavior, the
router, application auth contract, and response mapping will be updated with
the smallest implementation needed to pass. Relevant BCS HTTP adapter and
organization service tests will then be run.
