# Scriptorium book package format

Last reviewed: 2026-10-28

Normative spec for a Scriptorium **book package** (formatVersion 1). A package is a
self-contained directory that the app can open, verify, and re-locate on disk without
dangling references. The locked decisions here come from the
[M0 foundation plan](../plans/archive/completed/2026-08-23-m0-tauri-foundation.md); this doc is the
human-readable source of truth. Field *types* are normative in
[`book-format.schema.json`](book-format.schema.json) — when the two disagree, the schema
wins on types, this doc wins on rules and intent.

## Package layout

```
<book-id>/
├── meta.json          # required — see fields below
├── pages/             # required — page images, relative paths stored in meta.json
├── ocr/               # reserved — may be empty; text layer lands here in M2+
└── annotations.json   # reserved — empty form is `[]` (canonical); `{}` tolerated on read, never written
```

**Portability:** everything inside the package is addressed with **relative paths**.
`meta.json` MUST NOT contain absolute paths (no home directory, no root-anchored source
path). Page `file` values MUST be package-relative, use forward slashes only, and MUST NOT
contain `.` or `..` path segments or null bytes (enforced in the schema and in Rust
`BookMeta::validate`). A package directory can be moved or copied to another machine and
still load.

## meta.json fields (formatVersion 1)

| Field | Type | Required | Notes |
|---|---|---|---|
| `formatVersion` | number | yes | `1` for this spec |
| `id` | string | yes | stable book identifier **and** package directory name; lowercase slug (1–64 chars); no trailing `.`; not a Windows-reserved device name (`con`/`prn`/`aux`/`nul`/`com1`–`9`/`lpt1`–`9`, optional extension) |
| `title` | string | yes | display title (`maxLength: 4096`) |
| `createdAt` | string | yes | RFC 3339 / ISO 8601 date-time (enforced in schema + `BookMeta::validate`) |
| `updatedAt` | string | yes | RFC 3339 / ISO 8601 date-time (enforced in schema + `BookMeta::validate`) |
| `renderMode` | string | yes | `"scan"` default; `"text"` / `"hybrid"` reserved |
| `lastReadPage` | number | yes | 0-based index; loader MUST clamp or reject when `>= pages.length` |
| `rights` | string | yes | may be empty (`maxLength: 8192`) |
| `attribution` | string | yes | may be empty (`maxLength: 8192`) |
| `pages[]` | array | yes | one entry per page, in order (`minItems: 1`, `maxItems: 100000`) |

`pages[]` item:

| Field | Type | Required | Notes |
|---|---|---|---|
| `index` | number | yes | 0-based position; MUST equal array position (`pages[i].index == i`) |
| `file` | string | yes | relative path under the package (no `.` / `..` segments, no absolutes, no trailing `.`; Windows-reserved components + case-insensitive uniqueness enforced in `BookMeta::validate`) |
| `width` | number | yes | image width in px (`minimum: 1`) |
| `height` | number | yes | image height in px (`minimum: 1`) |
| `byteSize` | number | yes | bytes on disk (`exclusiveMinimum: 0`) |
| `sha256` | string | yes | hex digest of the page file |
| `pageLabel` | string | no | optional human label |
| `storage` | string | no | `"copied"` default; `"referenced"` reserved, not implemented in M1 |

## Planned: originals and display copies

M1 ingest accepts formats the webview cannot show, such as TIFF and JPEG 2000. It will store
each original beside a display copy. `file` stays the display image. A new optional page
object, `source` (`file`, `byteSize`, `sha256`), points at the original. Because this adds a
field, it ships as `formatVersion` 2 with a migration note. Input formats are listed in the
[M1 plan](../plans/2026-08-23-m1-scan-viewer.md#input-formats-normative-for-m1).

## Evolution rule

While `formatVersion` is `1`, the schema and Rust loaders reject unknown keys
(`additionalProperties: false` / `#[serde(deny_unknown_fields)]`). Therefore any
**new field** in `meta.json` (or nested page objects) REQUIRES bumping
`formatVersion` and a migration note — older v1 readers cannot ignore unknown keys.

Within a given `formatVersion`, do not rename or remove fields, and do not change
the meaning of existing fields. Forward-compatible additive extensions are only
allowed for future versions whose readers deliberately support them (for example
by dropping `deny_unknown_fields` or documenting an extension map). A breaking
change (rename, remove, or semantic change of an existing field) likewise requires
bumping `formatVersion` and a migration note describing how to upgrade packages.

## Loader validation

After JSON Schema validation, loaders MUST run the Rust `BookMeta::validate` checks (or
an equivalent):

| Check | Behavior |
|---|---|
| `id` | MUST be a portable library directory slug: lowercase `[a-z0-9][a-z0-9._-]{0,63}`, no trailing `.`, not a Windows-reserved device name (incl. `con.txt`-style extensions). |
| `title` / `rights` / `attribution` | MUST respect schema max lengths (4096 / 8192 / 8192). |
| `createdAt` / `updatedAt` | MUST parse as RFC 3339 date-time. |
| `lastReadPage` | MUST be `< pages.length`. Prefer reject (`LastReadPageOutOfRange`); UI may clamp to `pages.length - 1` if recovering a corrupt bookmark. |
| `pages` | MUST have `1..=100000` entries. |
| `pages[].index` | MUST equal array position (`pages[i].index == i`); reject duplicates / gaps / out-of-order. |
| `pages[].file` | Reject absolute paths, `.` / `..` segments, backslashes, null bytes, empty segments, trailing `.`, and Windows-reserved components (`con`/`prn`/`aux`/`nul`/`com1`–`9`/`lpt1`–`9`, optional extension, case-insensitive). |
| `pages[].file` (cross-page) | Reject two pages whose `file` paths collide case-insensitively (`pages/A.png` vs `pages/a.png`) — required for Windows/macOS default volumes; not expressible in JSON Schema. |
| `pages[].width` / `height` | Reject values `< 1` (degenerate images). |
| `pages[].byteSize` | Reject `0` (empty page files). |

### Page `file` schema pattern (invariants)

The JSON Schema `file` regex encodes these invariants (also enforced in Rust
`validate_page_file`):

1. Not absolute (`/` or `\` prefix); no backslashes anywhere.
2. No `.` or `..` path segments (including terminal).
3. No path segment ending with a trailing `.` (Windows-invalid), e.g. reject `pages/page.`.
4. Only ASCII alnum / `.` / `_` / `-` / `/` characters.

Windows-reserved device-name segments and case-insensitive collisions across pages are
**not** fully expressible in the pattern; `BookMeta::validate` rejects those.

## Ingest rules

Each ingest copies source page images into a new package under the library root
(`<app-data>/books/`). The rules below are load-bearing; the error names are what the
ingest pipeline returns on failure.

| Rule | Behavior on violation | Error |
|---|---|---|
| **Storage default** | Copy page bytes into `pages/`. Symlinks are rejected. `storage: "referenced"` is reserved and NOT implemented in M1. | — |
| **Source collisions** | If two source files map to the same destination stem, including case-insensitive match on APFS/NTFS (e.g. `1.jpg` + `1.JPG`), ingest fails — never silently overwrite. | `DuplicatePageStem` |
| **Atomic write** | Write to a temp dir under the destination parent → validate → rename into place. If a destination book id already exists, ingest fails. | `DestinationExists` |
| **Orphan cleanup** | On startup and before ingest, delete orphaned `*.scriptorium-tmp` / `.ingest-tmp-*` dirs under the library root older than 1 hour (or any left from a previous crashed run). | — |
| **Checksums** | Compute `sha256` at ingest and store it in `meta.json`. `load_book` re-verifies the sha256 of each page file; drift is a hard failure. Skip re-verify only behind an explicit future flag — not in M1. | `ChecksumMismatch` |
| **Free space** | Pre-flight check: refuse the copy if free space is less than `estimated_bytes × 1.1 + 50 MB` headroom. | — |

## Provenance and privacy

`meta.json` MUST NOT store an absolute `sourcePath`. The package is self-contained and
portable; binding it to a specific source location breaks that.

EXIF / XMP metadata embedded in source images is copied **bytes-as-is** in M1. Stripping
or policy (GPS removal, etc.) is deferred but documented here so a future ingest path can
add it without a schema change.
