<p align="center">
  <img
    src="assets/github/readme-header.png"
    alt="Hamstik CLI — build, automate, and integrate from your terminal"
    width="100%"
  />
</p>

Hamstik CLI is the official native command-line interface for
[Hamstik](https://hamstik.com), built for developers, automation, CI/CD, and
coding agents.

It talks to Hamstik exclusively through the documented **Hamstik Public API v1**
under `/api/v1`, making the same platform capabilities available to terminals,
scripts, and agent workflows.

> **Status:** Pre-alpha. The current Public API v1 command surface is implemented,
> but packaging and the first supported release are still under development.

[![CI](https://github.com/bbs-steven/hamstik-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/bbs-steven/hamstik-cli/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Quick start

The CLI is not yet distributed as an installable package. Build it from source
with a Rust toolchain:

```bash
git clone https://github.com/bbs-steven/hamstik-cli.git
cd hamstik-cli
cargo build
cargo run -p hamstik-cli -- --help
cargo run -p hamstik-cli -- --version
```

Once built, sign in and start working (a Personal Access Token is stored in the
OS credential store):

```bash
hamstik auth login --with-token      # reads the PAT from stdin
hamstik context init --org acme --project HAM
hamstik work list --mine
hamstik work start HAM-1
```

## Terminal banner

Running `hamstik` with no command — or with `--help` / `--version` — prints the
identity banner:

```text
              _                         _   _ _
             | |__   __ _ _ __ ___  ___| |_(_) | __
    (\___/)  | '_ \ / _` | '_ ` _ \/ __| __| | |/ /
    (='.'=)  | | | | (_| | | | | | \__ \ |_| |   <
    (")_(")  |_| |_|\__,_|_| |_| |_|___/\__|_|_|\_

🐹 hamstik cli v0.1.0      © Blackboard Studios LLC
```

The banner appears only on the human root help and version surfaces (`hamstik`,
`-h`/`--help`, `-V`/`--version`, and `version`). It is intentionally omitted
from `--json` machine output, subcommand help, and completion scripts.

## Why Hamstik CLI?

![Why Hamstik CLI?](assets/github/why-hamstik-cli.png)

- **API-first** — the CLI is built against the documented Hamstik Public API,
  not private server internals. What the API allows, the CLI does; what it
  doesn't, the CLI won't pretend to.
- **Automation-ready** — designed from the start for terminals, scripts,
  CI/CD pipelines, and coding agents, with a stable machine-facing contract in
  mind.
- **Native Rust binary** — no Node.js, Python, or other language runtime is
  required to build or run it.
- **Frozen API contract** — the repository carries a checked-in OpenAPI
  snapshot of Hamstik Public API v1 so CLI development has a reproducible
  reference as the platform evolves.

## How it works

![Terminal to Hamstik API workflow](assets/github/workflow-diagram.png)

```text
Terminal / automation / coding agent
              ↓
          Hamstik CLI
              ↓
      Hamstik Public API v1
              ↓
        Hamstik workspace
```

Hamstik CLI does not reach into Hamstik's database or application internals.
Everything goes through the documented Public API under `/api/v1`. The
checked-in OpenAPI snapshot gives CLI development a reproducible contract while
the live Hamstik service remains the authoritative implementation.

## Current capabilities

The Dogfooding Alpha is implemented. Today the CLI provides:

- a native Rust workspace that builds the `hamstik` executable;
- a typed client for the Hamstik Public API v1 (`/api/v1`) with pagination,
  retries, and idempotency-key support;
- authentication and profiles — `hamstik auth login|status|list|switch|logout|forget`,
  with secrets held only in the OS credential store;
- working context — `hamstik context show|set|clear|init` backed by a
  project-local `.hamstik.toml` plus global profile defaults;
- organizations and projects — `hamstik org ...` and `hamstik project ...`
  (`list`, `view`, `create`, `edit`, `archive`, `unarchive`, `activity`,
  `use`);
- member directory — `hamstik org members` and Organization-wide work via
  `hamstik org work` (`--mine` for the authenticated user);
- user profiles — `hamstik user view|work|activity|avatar` for
  visibility-scoped public data;
- authenticated identity via `hamstik me`, including credential scopes,
  expiration, default Organization, and memberships;
- sprints — `hamstik sprint list|view|create|transitions|transition` with
  completion actions for sprints that still have unfinished work items;
- labels — `hamstik label list|create` and `hamstik work label add|remove`
  (attach/detach with Work Item revision protection);
- work items — `hamstik work list|view|create|edit`, status transitions
  (`transitions`, `transition`, `start`, `close`), comments
  (`work comment list|add|edit|delete`), links
  (`work link list|add|delete`), activity (`work activity`), archive/
  unarchive/delete lifecycle, and bulk operations
  (`work bulk create|update|transition`);
- first-class My Work via `hamstik work mine` (`work my` alias) and
  Organization-wide SqueakQL search via `hamstik work search` /
  `hamstik squeakql validate`;
- attachments — `work attachment list|upload|download|delete`;
- the unauthenticated live contract via `hamstik api openapi`;
- automation-friendly output via `--json` / `--quiet` and stable exit codes;
- shell completions (`hamstik completion <shell>`), connectivity
  diagnostics (`hamstik doctor`) including terminal rendering checks
  (color/emoji probes with visual samples), and ANSI-free `--json` output;
- cross-platform CI on Linux, Windows, and macOS.

OAuth, the MCP server, and the Agent Skill remain future work. See the
[design documents](#design-documents) for where the CLI is headed.

## Command examples

Global `--org`, `--project`, and `--json` options may be placed before or after
subcommands. Cursors are opaque: pass the returned `nextCursor` unchanged.

```bash
# Authentication, identity, and context
hamstik auth login --with-token
hamstik me --json
hamstik context init --org acme --project HAM

# Organizations, members, Projects, and Sprints
hamstik org list --all
hamstik org members acme --search steven
hamstik project create --name "Website" --key WEB --color '#3b82f6'
hamstik project edit WEB --description "Public site"
hamstik sprint create --name "September" --goal "Ship v1" --target-points 40
hamstik sprint transitions 11111111-1111-4111-8111-111111111111
hamstik sprint transition 11111111-1111-4111-8111-111111111111 active

# Structured Work Item filters, Organization Work, and My Work
hamstik work list --status todo --status in_progress --label-name api \
  --sprint none --top-level --sort dueDate --fields title,status,dueDate
hamstik org work --project HAM --priority urgent --overdue true
hamstik work mine --scope open --project HAM --project WEB --all

# SqueakQL validation and read-only JSON-body search
hamstik squeakql validate 'status = todo and priority >= high'
hamstik work search 'status = todo and priority >= high' --limit 100 --json

# Work Item lifecycle and optimistic concurrency
hamstik work create --title "Document API" --type task --assignee me
hamstik work edit HAM-42 --priority high --parent HAM-7
hamstik work transitions HAM-42
hamstik work transition HAM-42 in_progress
hamstik work archive HAM-42
# --force is explicit last-write-wins (If-Match: *); it is never the default.
hamstik work unarchive HAM-42 --force

# Bulk request files map directly to the Public API operation-array schemas
hamstik work bulk create --operations-file create-operations.json --json
hamstik work bulk update --operations-file update-operations.json \
  --concurrency require-revision --json
hamstik work bulk transition --operations-file transition-operations.json \
  --concurrency last-write-wins --json

# Labels, links, threaded comments, and attachments
hamstik label create --name api --color '#6366f1'
hamstik work label add HAM-42 --label api
hamstik work link add HAM-42 --target-key WEB-9 --relation blocks
hamstik work comment add HAM-42 --body "Ready for review"
hamstik work comment add HAM-42 --body "Agreed" --parent COMMENT_UUID
hamstik work attachment upload HAM-42 ./design.png --content-type image/png
hamstik work attachment download HAM-42 ATTACHMENT_UUID --output ./design.png

# Public user profile resources use immutable usr_ identifiers
hamstik user view usr_cPbfeqnghA-RLpDVOMQhHg
hamstik user work usr_cPbfeqnghA-RLpDVOMQhHg --involvement created
hamstik user activity usr_cPbfeqnghA-RLpDVOMQhHg --since 2026-09-01T00:00:00Z
hamstik user avatar usr_cPbfeqnghA-RLpDVOMQhHg --format webp --output avatar.webp
```

Downloads always write binary data to a file. In `--json` mode stdout contains
only JSON metadata (path, size, content type, and available response headers),
never the binary payload. Diagnostics and structured failures go to stderr.

## Credentials, profiles, and logout

`hamstik auth login` validates the PAT against `GET /api/v1/me` before storing
anything. Two things are then kept in two different places:

| What | Where |
| --- | --- |
| The PAT itself | OS credential store — Windows Credential Manager, macOS Keychain, or the Linux Secret Service (GNOME Keyring / KWallet) |
| Profile metadata (host, user id, email, defaults) | `config.toml` next to the other configuration (`hamstik auth list` reads this) |

The CLI never falls back to plaintext storage. If no secure credential store is
reachable — a headless Linux box with no Secret Service, for example — every
login/logout fails with an explanation instead of writing the token to disk; use
`HAMSTIK_TOKEN` for headless automation (see SPEC §26). `hamstik doctor` reports
whether a persistent store was found under the `credential store` check.

`hamstik auth logout` is a **local logout**: it removes the stored PAT for the
selected profile and does not revoke the token server-side (the Public API has
no PAT-revocation endpoint yet). Profile metadata is intentionally kept, so
`hamstik auth list` still shows the profile after logout and `auth status`
reports `no stored credential` until you log in again. Logging out twice is
idempotent and reports `already logged out`.

To remove a profile entirely — credential *and* its `config.toml` entry — use
`hamstik auth forget [PROFILE]` (the selected profile when no name is given).
This is still local only: the PAT remains valid server-side, so revoke it in the
Hamstik web UI if that matters. If the profile being forgotten was active, the
slot is refilled only when exactly one profile remains; otherwise no profile is
active until `auth login` or `auth switch` picks one, because guessing would
silently change which host your commands talk to. If the credential store is
unreachable, the profile entry is still removed and the exact account key is
printed so the orphaned secret can be deleted by hand.

### When the configuration file is broken

A missing `config.toml` is normal and treated as an empty configuration. A file
that exists but cannot be understood (malformed TOML, an unknown field, an
unsupported schema version, unreadable permissions) is always an error, never
silently ignored — that would discard profiles and context defaults. The failure
names the offending file, points at a repair or removal hint, and exits with the
configuration exit code (10). `hamstik doctor` reports the same problem as a
`FAIL` line instead of refusing to run, so the rest of the diagnostics —
including the credential store check, which does not depend on config — stay
available. The same applies to a broken project-local `.hamstik.toml`.

## Build from source

Prerequisites:

- Git
- A Rust toolchain — install it with [rustup](https://rustup.rs/); the
  repository pins its toolchain through `rust-toolchain.toml`, which `cargo`
  picks up automatically

No Node.js, Python, or other language runtime is required.

```bash
cargo build --workspace
cargo test --workspace
cargo run -p hamstik-cli -- --help
```

## Repository structure

```text
.github/       CI workflows and repository governance
assets/        README, social-preview, and mascot artwork
crates/        Rust workspace (hamstik-cli, hamstik-api-client)
design/        Product requirements, technical specification, versioning policy
openapi/       Frozen Hamstik Public API v1 OpenAPI snapshot
skills/        Hamstik Agent Skill materials
CHANGELOG.md   Release notes (Keep a Changelog format)
```

## API contract

Hamstik CLI uses only the public API under `/api/v1`. The checked-in
[`openapi/hamstik-v1.json`](openapi/hamstik-v1.json) is the frozen development
snapshot of the Hamstik Public API v1 contract used for CLI development and
contract testing — see [`openapi/README.md`](openapi/README.md) for how it is
maintained.

[`openapi/api-parity.json`](openapi/api-parity.json) accounts for every
`operationId` with its API-client method, CLI command, and test classification.
The parsed parity test fails on unclassified operations. Developers can run
`scripts/update-openapi.sh --check` to report live drift or `--update` to
deliberately refresh the byte-for-byte snapshot; normal builds remain offline.

CLI code must not silently depend on private Hamstik web-app or server
internals, and contract changes are synchronized intentionally rather than
casually.

## Development

Quality gates, matching CI exactly:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Tests use Rust's built-in test framework. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the full development setup.

## Design documents

The detailed product and technical direction lives in:

- [design/PRD.md](design/PRD.md) — product requirements
- [design/SPEC.md](design/SPEC.md) — technical specification
- [design/VERSIONING.md](design/VERSIONING.md) — semantic-versioning policy
  (how MAJOR/MINOR/PATCH are chosen)

Notable changes are tracked in [CHANGELOG.md](CHANGELOG.md) following the
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.

These documents are authoritative for the CLI's architecture and command
surface. This README intentionally stays higher level.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Coding
agents must read [AGENTS.md](AGENTS.md) before making changes.

## Security

Do not report vulnerabilities in public GitHub issues. See
[SECURITY.md](SECURITY.md) for the private reporting process.

## License

Hamstik CLI is open-source software licensed under [Apache-2.0](LICENSE).

The Hamstik service and web application are separate products and are not
licensed under this repository's Apache-2.0 license.

## Trademark

Hamstik and the Hamstik logo are trademarks of Blackboard Studios.
