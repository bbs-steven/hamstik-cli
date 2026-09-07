# Hamstik CLI
## Versioning Policy

**Status:** Adopted
**Related:** `design/PRD.md` (§40 Versioning), `design/SPEC.md`, `CHANGELOG.md`
**Applies to:** the `hamstik` binary and both workspace crates
(`hamstik-cli`, `hamstik-api-client`, versioned in lockstep)

---

# 1. Scheme

Hamstik CLI follows [Semantic Versioning](https://semver.org/) — `MAJOR.MINOR.PATCH`
— and documents every release in [`CHANGELOG.md`](../CHANGELOG.md) following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

```text
MAJOR.MINOR.PATCH
  │      │     └─ compatible bug fixes
  │      └─────── backwards-compatible additions
  └────────────── breaking changes
```

Versioning is independent from Hamstik server releases (PRD §40). A CLI
`1.4.2` may talk to any Hamstik server implementing Public API v1; the CLI
MUST NOT gate on the server's application version.

Single version source: the workspace `Cargo.toml` `[workspace.package]`
version, shared by both crates and baked into `--version` / `version` output
and the `User-Agent` header. Release automation changes it in exactly one
place.

---

# 2. Major — breaking changes

Increment MAJOR when a user could follow documented behavior, upgrade the
CLI, and have a script, alias, or agent workflow stop working.

Breaking changes include:

- removing a command, subcommand, or flag;
- renaming commands, flags, or output fields;
- changing a stable exit code (`SPEC §52`) or the JSON failure schema
  (`SPEC §54`);
- changing the `--json` success envelope shape of an existing resource;
- changing the meaning of an existing flag (value semantics, defaults,
  accepted units);
- changing the context/config file schema without a supported migration
  (breaking `SPEC §22`, `§32`);
- changing the credential store key format such that existing stored
  credentials become unreadable (`SPEC §24`);
- removing support for a documented platform;
- requiring a Public API version newer than the one documented as supported
  (`SPEC §58`).

During pre-1.0 (see §7) MAJOR stays `0` and these changes land as MINOR
bumps with `### Breaking` subsections in the changelog.

Rules:

- One MAJOR release must not bundle unrelated breaking changes; each
  breaking change needs its own changelog entry with a migration note.
- Breaking changes to `--json` output require at least one MINOR release of
  deprecation warning before removal, whenever a warning is technically
  possible on the client side.
- The frozen OpenAPI snapshot (`openapi/hamstik-v1.json`) documents which
  server contract a given CLI release targets; a snapshot refresh that
  removes or reshapes consumed endpoints is a breaking CLI change unless the
  CLI keeps working against servers without the new surface.

---

# 3. Minor — backwards-compatible additions

Increment MINOR when users gain capabilities and existing documented behavior
is unchanged.

Minor changes include:

- new commands or subcommands (e.g. `sprint`, `label`, `work attachment`);
- new flags on existing commands, including new list filters;
- new fields in `--json` output (additive; consumers tolerate unknown
  fields);
- new informational `doctor` checks or sample lines;
- new color/output decorations in human mode that degrade cleanly when color
  is disabled;
- adopting a newly added Public API v1 endpoint or error code;
- performance improvements, new platform support, dependency upgrades that
  add capability.

Rules:

- Additive JSON fields must be optional to consume: absent/never-present
  handling is the consumer's contract, and existing consumers must not break.
- Human-readable output may change layout freely in MINOR releases as long
  as identifiers rendered in `--quiet` mode remain stable (quiet output is a
  documented machine surface; column additions to tables are not breaking).
- A MINOR release may include PATCH-level fixes; the changelog documents
  them under the same version.

---

# 4. Patch — compatible fixes

Increment PATCH when behavior is wrong relative to documentation or intent,
and fixing it does not change documented contracts.

Patch changes include:

- bug fixes (wrong exit code, crash, mangled table alignment, incorrect
  JSON encoding);
- corrected error mapping that aligns the CLI with documented behavior
  (e.g. an error code mapped to the wrong exit code class);
- terminal rendering fixes (colors, emoji, width) that make output correct
  on previously broken terminals;
- documentation corrections, dependency updates with no behavioral change,
  build/CI fixes.

Rules:

- A fix that changes *documented* behavior to something new is not a patch —
  it is MINOR (addition/change) or MAJOR (breaking).
- Security fixes are PATCH (or MINOR/MAJOR if the fix itself changes
  contracts), always with a changelog entry; embargoed details stay out of
  the public changelog until disclosure.

---

# 5. What does not bump the version

- Repository-internal work with no user-visible effect: CI changes, test
  additions, refactors with identical behavior, design document updates,
  agent-only scripts.
- These still get changelog entries under a `### Internal` heading when the
  release ships any user-visible change, so the release narrative is
  complete.

---

# 6. Pre-1.0 rules (0.x)

Per PRD §40, dogfooding uses `0.x`. While MAJOR is `0`:

- **MINOR (0.x.0)** may contain breaking changes. Treat every `0.x.0` bump
  as potentially breaking; read the `### Breaking` section before upgrading.
- **PATCH (0.x.y)** remains compatible-only (bug fixes, tiny additive
  polish that cannot break anyone).
- Stability of a documented contract (exit codes, JSON schema, command
  grammar) is signaled by the changelog, not by the version number.

`1.0.0` marks: stable core command contracts, stable JSON output,
established installation channels, Agent Skill support, and API
compatibility proven through dogfooding (PRD §40).

---

# 7. Choosing between PATCH and MINOR — decision procedure

Ask, in order:

1. **Did any documented behavior change for an existing user?**
   No → PATCH (or nothing, §5). Yes → step 2.
2. **Can an existing script break?** (removed/renamed surface, changed
   output, changed exit code, changed default)
   Yes → MAJOR (or MINOR with `### Breaking` while 0.x). No → step 3.
3. **Did the user gain something?** (new command/flag/field/check)
   Yes → MINOR. No → PATCH.

Edge cases:

- **Fix + feature in one release:** bump to the higher of the two
  requirements (MINOR), with separate changelog sections.
- **New optional flag on an existing command:** MINOR (additive).
- **Flag accepts a new value where unknown values previously errored:**
  MINOR; if the CLI previously succeeded on that value with different
  behavior, MAJOR.
- **OpenAPI snapshot refresh that only adds endpoints the CLI does not
  call:** no bump until the CLI adopts them (then MINOR).
- **OpenAPI snapshot refresh consumed by the CLI (new commands using new
  endpoints):** MINOR.
- **Dependency security update forcing behavior-compatible changes:** PATCH.
- **Internal crate (`hamstik-api-client`) changes:** versioned in lockstep
  with the CLI binary; its changelog entries live in the same release
  section.

---

# 8. Changelog discipline

`CHANGELOG.md` at the repository root, Keep a Changelog format:

- An `## [Unreleased]` section accumulates changes as they land, grouped by
  `### Added` / `### Changed` / `### Deprecated` / `### Removed` /
  `### Fixed` / `### Security` (and `### Breaking` subsections where
  needed).
- Every user-visible change gets an entry at merge time, written for a user
  of the CLI ("Add `sprint transition`" not "implement handler for X").
- Releases rename `## [Unreleased]` to `## [VERSION] - YYYY-MM-DD`, link it
  in the reference list, and open a fresh `## [Unreleased]`.
- The version in `Cargo.toml` and the newest changelog release section must
  always agree.

---

# 9. Release checklist

1. Ensure `## [Unreleased]` covers everything since the last release (for
   the first release: everything, since nothing has shipped yet);
   prune `### Internal` noise or keep it, per audience.
2. Bump the workspace version (`Cargo.toml`), run the quality gates
   (AGENTS.md): `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets --all-features -- -D warnings`, `cargo test --workspace`,
   `cargo build --workspace --release`, `git diff --check`.
3. Rename the changelog section to the new version with the release date.
4. Tag `vMAJOR.MINOR.PATCH` (annotated) on the release commit.
5. Packaging, signing, and channel distribution follow PRD §39; this
   document does not restate them.