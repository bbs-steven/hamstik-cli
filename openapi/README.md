# Hamstik Public API v1 contract snapshot

`hamstik-v1.json` is a **frozen development snapshot** of the Hamstik Public API v1
OpenAPI 3.1.1 contract. It exists to support Hamstik CLI development and contract
testing.

## Authority

The authoritative runtime contract is always the live server document at:

```text
https://www.hamstik.com/api/v1/openapi.json
```

Do not treat this snapshot as more authoritative than the server implementation. Additive
server changes are expected; the snapshot is refreshed intentionally when the Public API
contract changes in a way the CLI must track.

## Purpose

- development reference for implementing the typed Rust API client;
- contract testing against expected operation inventory;
- detecting unexpected client/server drift.

`api-parity.json` is the machine-readable operation support manifest. The
`openapi_parity` integration test parses both JSON documents and fails whenever
an `operationId` is added, removed, moved, or changed without a deliberate
client/CLI/test classification.

## Checking and updating

Live access is explicit and never part of an offline build:

```bash
scripts/update-openapi.sh --check
scripts/update-openapi.sh --update
cargo test -p hamstik-api-client --test openapi_parity
```

`--check` reports drift without changing the working tree. `--update` first
downloads and validates the live JSON, then replaces the snapshot byte-for-byte;
the snapshot is never edited by hand. After an update, implement every new or
changed operation and update `api-parity.json` in the same change.

## Rules

- Refresh `hamstik-v1.json` deliberately, never casually.
- Download the snapshot; do not manually edit it.
- CLI builds must not fetch OpenAPI from the network.
- No OpenAPI code generation runs against this snapshot; the design calls for a
  deliberate hand-written typed client.
