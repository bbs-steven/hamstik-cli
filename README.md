# Hamstik CLI

Hamstik CLI is the official native command-line interface for [Hamstik](https://hamstik.com).
It is intended for developers, automation, CI/CD, and coding agents.

**Status: Pre-alpha / under active development.**

This repository currently contains the project foundation: design documents, a Rust
workspace, governance documentation, CI, and a frozen snapshot of the Hamstik Public
API v1 contract. The functional Dogfooding Alpha implementation is forthcoming.

## About Hamstik

Hamstik is the project tracking and collaboration service at
[hamstik.com](https://hamstik.com). This CLI talks to Hamstik exclusively through its
documented Public API under `/api/v1`.

## Build from source

You need a Rust toolchain (see [CONTRIBUTING.md](CONTRIBUTING.md)). No Node.js, Python,
or other language runtime is required.

```bash
cargo build
cargo test
cargo run -p hamstik-cli -- --help
cargo run -p hamstik-cli -- --version
```

## Repository structure

```text
.github/    CI workflows and governance configuration
crates/     Rust workspace (hamstik-cli, hamstik-api-client)
design/     Product requirements and technical specification
openapi/    Frozen Hamstik Public API v1 OpenAPI snapshot
skills/     Future home of the canonical Hamstik Agent Skill
```

## Design documents

- [design/PRD.md](design/PRD.md) — product requirements
- [design/SPEC.md](design/SPEC.md) — technical specification

These documents are authoritative for the CLI's product direction and architecture.

## Development quality commands

These checks match CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Security issues
are handled privately — see [SECURITY.md](SECURITY.md). Coding agents should read
[AGENTS.md](AGENTS.md) before making changes.

## License

Hamstik CLI is open-source software licensed under [Apache-2.0](LICENSE).

The Hamstik service and web application are separate products and are not licensed
under this repository's Apache-2.0 license.

## Trademark

Hamstik and the Hamstik logo are trademarks of Blackboard Studios.
