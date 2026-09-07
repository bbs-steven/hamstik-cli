# AGENTS.md — Guidance for Coding Agents

This repository is the official public repository for the Hamstik CLI. If you are a
coding agent (or a human following agent-style automation), these rules govern your
behavior here. They exist so agents do not make fundamental architectural decisions on
their own.

## Read the design first

Before meaningful implementation work, read:

- [design/PRD.md](design/PRD.md) — product requirements
- [design/SPEC.md](design/SPEC.md) — technical specification

These documents are authoritative. Do not contradict them. Do not invent architecture
that conflicts with them. If something in the design appears wrong or outdated, raise it
instead of silently deviating.

## Public repository

Everything committed here must be safe for public disclosure. Never copy proprietary
Hamstik server source, internal documentation, or private credentials into this
repository.

## API boundary

The CLI may only use Hamstik's documented Public API:

```text
/api/v1
```

Never use private Hamstik browser routes (`/api/*` routes not part of the Public API v1
contract). The frozen contract snapshot lives in `openapi/hamstik-v1.json` with
explanations in `openapi/README.md`.

## No server business logic duplication

The Hamstik server remains authoritative for:

- authorization;
- Work Item status transitions;
- validation;
- ETags/revisions;
- idempotency;
- Organization isolation.

The CLI provides ergonomics around those rules. Do not recreate or second-guess server
business logic in client code.

## No secrets

Never commit:

- Personal Access Tokens (PATs);
- `Authorization` headers or captured HTTP traffic containing credentials;
- passwords;
- local credentials or real `.env` files;
- private API keys.

If you encounter a secret in the working tree, do not commit it and report it.

## Native CLI in Rust

The official CLI is Rust. Do not create a second implementation in TypeScript, Python,
Go, or shell. Development automation scripts are fine, but the product CLI remains a
single native Rust binary.

## Cross-platform

Windows, macOS, and Linux are first-class. Do not introduce platform assumptions that
break any of them. Prefer platform-neutral Rust abstractions; isolate platform-specific
behavior behind abstractions when necessary.

## Quality gates

Before declaring any change complete, all of the following must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
git diff --check
```

## No telemetry

Do not introduce usage telemetry or hidden network calls without an explicit product
decision documented in the design files.

## Documentation

User-visible CLI changes require corresponding documentation changes. Canonical
end-user CLI documentation ultimately lives in the main Hamstik application
documentation repository/tree. This repository owns implementation/developer
documentation and the future Agent Skill. Include documentation updates in the
same change when behavior changes.

## Changelog and versioning

Every user-visible change gets an entry in [CHANGELOG.md](CHANGELOG.md) under
`## [Unreleased]` at merge time, and version bumps follow
[design/VERSIONING.md](design/VERSIONING.md). Breaking changes to documented
surfaces (commands, flags, JSON output, exit codes) are called out in a
`### Breaking` subsection.

## Agent Skill

The canonical Hamstik Agent Skill will live in `skills/` of this repository once the
relevant CLI command surface exists. Do not create or substantially modify the skill
until the commands it describes actually exist.

## Dependencies

- Prefer mature, widely used Rust ecosystem dependencies.
- Do not add multiple libraries for the same concern.
- Dependencies must be compatible with Apache-2.0 distribution.
- Do not add dependencies "for later" — add them when actually used.

## Scope discipline

This repository is being implemented in stages. Do not jump ahead to packaging, release
automation, OAuth, MCP, or other future milestones unless the current task explicitly
calls for it.