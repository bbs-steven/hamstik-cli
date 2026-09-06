# Hamstik Public API v1 contract snapshot

`hamstik-v1.json` is a **frozen development snapshot** of the Hamstik Public API v1
OpenAPI 3.1.1 contract. It exists to support Hamstik CLI development and contract
testing.

## Authority

The authoritative runtime contract is always the live server document at:

```text
https://hamstik.com/api/v1/openapi.json
```

Do not treat this snapshot as more authoritative than the server implementation. Additive
server changes are expected; the snapshot is refreshed intentionally when the Public API
contract changes in a way the CLI must track.

## Purpose

- development reference for implementing the typed Rust API client;
- contract testing against expected operation inventory;
- detecting unexpected client/server drift.

## Rules

- Refresh `hamstik-v1.json` deliberately, never casually.
- CLI builds must not fetch OpenAPI from the network.
- No OpenAPI code generation runs against this snapshot; the design calls for a
  deliberate hand-written typed client.