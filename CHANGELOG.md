# Changelog

All notable changes to the Hamstik CLI are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/) as detailed in
[`design/VERSIONING.md`](design/VERSIONING.md). While the CLI is `0.x`,
MINOR releases may contain breaking changes — read the *Breaking* notes
before upgrading.

No release has been published yet: everything below is part of the
in-progress first release and will be dated and versioned when it ships.

## [Unreleased]

### Added

- Typed API client crate (`hamstik-api-client`) with bearer injection,
  retry policy with `Retry-After` awareness, idempotency-key generation,
  ETag capture, cursor pagination helpers, and multipart upload support.
- Frozen Hamstik Public API v1 OpenAPI snapshot (`openapi/hamstik-v1.json`)
  as the development contract.
- `auth login --with-token`, `auth status`, `auth list`, `auth switch`,
  `auth logout`, and `auth forget` with PAT storage in the OS credential
  store (Windows Credential Manager, macOS Keychain, Linux Secret Service);
  headless automation via the `HAMSTIK_TOKEN` environment variable (`auth`).
- Working-context management: `context init`, `context show`, `context set`,
  `context clear`, backed by a project-local `.hamstik.toml` plus global
  profile defaults (`context`).
- Organization and project commands: `org list|view|use`,
  `project list|view|create|use` (`org`, `project`).
- Work item commands: `work list|view|create|edit`, status transitions
  (`work transitions`, `work transition`, `work start`, `work close`),
  comments (`work comment list|add|delete`), labels
  (`work label add|remove`), and attachments
  (`work attachment list|upload|download|delete`) (`work`).
- Sprint commands: `sprint list|view|create|transitions|transition` with
  completion actions (`--move-to-backlog` / `--move-to-sprint`) and Sprint
  ETag handling (`sprint`).
- Project label management: `label list|create` (`label`).
- Work item list filters, including `label`, `labelName`, `parent`,
  `topLevel`, `sprint`, and `updatedAfter`.
- Machine-facing output: `--json` / `--quiet` global modes, stable exit
  codes (SPEC §52), raw API body echo in `--json`, and an ANSI-free JSON
  failure envelope on stderr.
- Connectivity diagnostics: `hamstik doctor` reporting configuration,
  credential store, authentication health, and terminal rendering probes
  (color and emoji, with visual sample lines) so users can confirm output
  will not be mangled on their terminal (`doctor`).
- Shell completion scripts via `hamstik completion <shell>` (`completion`).
- Terminal identity banner on root help and version surfaces (`version`).
- Project and label colors render as bracketed swatches (`[██]`) in
  `project list`, `project view`, `label list`, and label detail views. The
  block glyphs are painted in the resource's actual color (truecolor with an
  xterm-256 fallback); the bracket frame and hex text stay in plain
  foreground so extreme colors such as black-on-black remain legible.
  Color-disabled and `--json` output are unchanged.

### Changed

- `Me` identity output includes `publicId` and per-Organization `username`;
  `auth status` reports both in human and JSON output.
- Authentication field names match the deployed API specification
  (`authentication` context in `GET /me`).
- Table rendering pads columns by visible width, so cells containing ANSI
  escape sequences (color swatches) no longer break column alignment.

[Unreleased]: https://github.com/bbs-steven/hamstik-cli/commits/main