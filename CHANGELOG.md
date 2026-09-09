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

- Full support for the current 53-operation Hamstik Public API v1 contract,
  including `me`, first-class `work mine` / `work my`, SqueakQL search via
  `work search`, independent `squeakql validate`, and unauthenticated
  `api openapi` contract retrieval.
- Parsed OpenAPI parity enforcement through `openapi/api-parity.json`; adding,
  removing, or moving an operation now fails a test until client, CLI, and test
  support are deliberately classified.
- `scripts/update-openapi.sh --check|--update` for explicit live-contract drift
  checks and validated byte-for-byte snapshot refreshes without making offline
  builds network-dependent.
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

- Refreshed the frozen OpenAPI snapshot from the live authoritative contract
  (51 → 53 operations) and aligned required/nullable response fields, bulk
  request operation types, profile avatar selectors, Work Item parent input,
  and Work Item label assignment by id or name.
- My Work and user-profile Work commands expose every filter defined by their
  respective current operations, including repeated array parameters, sparse
  fields, due-date filters, archived state, and opaque cursors.
- Binary download metadata now preserves content type, content length,
  content disposition, cache control, and request id where the operation
  defines them; JSON output contains metadata only, never binary bytes.
- API errors retain `fieldErrors`, `details`, and request ids. Human errors
  always print the correlation id when available; JSON preserves the complete
  structured error information.
- `Me` identity output includes `publicId` and per-Organization `username`;
  `auth status` reports both in human and JSON output.
- Authentication field names match the deployed API specification
  (`authentication` context in `GET /me`).
- Table rendering pads columns by visible width, so cells containing ANSI
  escape sequences (color swatches) no longer break column alignment.

### Fixed

- `work edit --parent` and `work create --parent` forward the current API's
  bounded parent identifier directly so the server remains authoritative for
  identifier resolution and validation (`work`).
- `work label add|remove --label` accepts a label name (case-insensitive;
  labels are stored lowercase) in addition to a UUID. Attach-by-name uses the
  current request body directly; detach-by-name resolves the id required by
  the DELETE path (`work label`).
- `--assignee me` on `work create|edit` resolves to the caller's `usr_` public
  ID via `GET /me` before sending; the server only accepts ids on writes
  (`work`).
- `user view|work|activity|avatar` accept `me` as the target, resolving it the
  same way (`user`).
- `--host` no longer hides a profile's stored credential: the lookup tries the
  selected host first, then the profile host recorded at login, and the
  failure message names both hosts when both were tried (`auth`, `doctor`).
- Human API errors always include `requestId`; `--verbose` additionally shows
  the HTTP status, matching the structured information retained by `--json`
  (`output`).

### Added

- `work comment list --exclude-deleted` hides soft-deleted comments instead of
  rendering `(deleted)` placeholders (`work comment`).
- The API client captures the documented `RateLimit-Limit` /
  `RateLimit-Remaining` / `RateLimit-Reset` headers on responses and errors.
  A successful request that leaves the window depleted waits out the reset
  (capped at the same 30 s bound as `Retry-After`) instead of sending the
  next request straight into a guaranteed `429`.

### Changed (API sync)

The frozen OpenAPI snapshot was refreshed against the updated Hamstik Public
API v1 (29 → 51 operations; all changes additive):

- Project lifecycle: `project edit`, `project archive`, `project unarchive`,
  and `project list --archived`; Project projections now carry `revision` and
  `archivedAt`, and edits use the `project-N` ETag (`project`).
- Work Item lifecycle: `work archive`, `work unarchive`, and
  `work delete [--cascade]` with Work Item ETag protection (`work`).
- Work Item links: `work link list|add|delete` with the `blocks`,
  `blocked_by`, and `relates` relations; new 409 codes `LINK_DUPLICATE` and
  `LINK_CONTRADICTION` map to the conflict exit code (`work link`).
- Activity feeds: `work activity`, `project activity`, and
  `user activity`, all paginated (`--since` supported) (`work`, `project`,
  `user`).
- Comment editing: `work comment edit` (author-only; responses carry
  `editedAt`) (`work comment`).
- Bulk operations: `work bulk create|update|transition` accepting 1–50
  operations per request from a JSON file or stdin, with per-item embedded
  results and `require-revision` / `last-write-wins` concurrency modes
  (`work bulk`).
- Member directory: `org members` over `GET /organizations/{slug}/users`
  (`org members`).
- Organization Work and My Work collections: `org work` lists Work Items
  across the organization with Project context; `--mine` sends
  `assignee=me` (`org work`).
- User profiles: `user view`, `user work`, `user activity`, and
  `user avatar` over the `profile:read` endpoints; profiles expose only
  public data (`publicId`, shared Organization usernames, visibility-scoped
  stats) (`user`).
- Work Item filters extended with `overdue`, `dueBefore`, `dueAfter`,
  `sort` (`updated|dueDate|priority|rank`), `archived`, and sparse
  `fields` fieldsets on `work list` and `org work`.
- Assignees now accept immutable public IDs (`usr_...`) on create/edit,
  mapped to `assigneePublicId`; legacy UUIDs keep using `assigneeId`.

### Changed (API sync)

- `user work` decodes the dedicated profile Work projection
  (`ProfileWorkItemList`) and gained a `REPORTER` table column; the
  reporter is rendered from the complete projection and shows `-` when
  absent or when a sparse fieldset omits it.
- Refreshed the frozen OpenAPI snapshot (`openapi/hamstik-v1.json`) for
  the profile Work projection change.
- Work Item list summaries are sparse-tolerant: only `id`, `key`, and
  `revision` are guaranteed when `fields=` is set; table rendering falls
  back per column.
- User summaries arrive in two shapes (`{id, name}` legacy and
  `{publicId, name}` public); the client decodes both, so `work archive`,
  `work unarchive`, label attach/detach, and comment edit responses render
  correctly.
- Work Item delete requests send an explicit `{"cascade": ...}` body and
  require the `work-item:delete` scope (Organization owners only).
- Error exit mapping: `LINK_DUPLICATE`, `LINK_CONTRADICTION`,
  `WORK_ITEM_ARCHIVED`, `WORK_ITEM_HAS_CHILDREN`, and `PROJECT_ARCHIVED`
  map to the conflict exit code (6).

### Breaking (API sync)

- `work list` gained shared filter flags via a flattened group; existing
  flag names and semantics are unchanged.
- `project list` output gained a `STATE` column; `project view` gained
  `revision` and `archived` detail lines (`0.x`, pre-release).

[Unreleased]: https://github.com/bbs-steven/hamstik-cli/commits/main
