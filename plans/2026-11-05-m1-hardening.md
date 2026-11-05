# Plan: M1 hardening (CI build, acceptance tests, asset scope, page labels, jsonschema)

Last reviewed: 2026-11-05
Date: 2026-11-05
Author: planning pass (branch `chore/m1-hardening`)
Status: in-progress
Linked issue/PR: n/a
Follows: [M1 follow-ups](2026-11-02-m1-followups.md)

Chunk 0 registered this file in `to_do.md` (**Next Up** and **Active plans**) and in
[`plans/README.md`](README.md). Do not rewrite checkboxes in
[`archive/completed/2026-08-23-m1-scan-viewer.md`](archive/completed/2026-08-23-m1-scan-viewer.md);
that file stays historical.

## Goal

Close the agent-doable hardening left after 0.2.0: catch a broken Tauri embed in CI,
lock the acceptance checks that already exist, record the asset-protocol verdict,
bound `pageLabel`, and drop the HTTP stack `jsonschema` links only to resolve remote
schemas. Readers get a smaller binary and a schema that rejects an unbounded label.
Maintainers get a CI job that fails when `frontendDist` or the app icons are wrong.

## Out of scope

- The manual script in [`docs/manual-test-m1.md`](../docs/manual-test-m1.md), including
  the release-build CSP console check (section 8), the Windows WebView2 run, and the
  Dulac A9 measurements. Those stay open on the follow-ups plan.
- ADR-0001's open platform items, making `rust-cross` required, and the layout-default
  decision.
- A `formatVersion` bump, showing `pageLabel` in the reader, or forcing Windows asset
  URLs onto `https://`.
- The openjpeg-sys patch, reader-note follow-ups, the decoded-scan cap, the grey+alpha
  JPEG 2000 fixture, and the Sonar backlog in hook and CI scripts.
- A `VERSION` bump. Land notes under `## [Unreleased]`. The release that includes this
  work is a **PATCH** (stricter validation and a smaller dependency graph, no new feature).

## Findings (decisions)

### 1. Debug `cargo build` must opt into `custom-protocol` (LC-M2)

Tauri 2.12's default features do not include `custom-protocol`. `src-tauri/Cargo.toml`
enables only `protocol-asset`. With that default, `tauri::generate_context!()` is a dev
build (`devUrl` is set), so it embeds no frontend. `cargo check`, `cargo clippy`,
`cargo test`, and a plain `cargo build` all succeed when `dist/` is missing. That is
why CI is green today, and why a bare `cargo build` step would not catch the failure
LC-M2 is about.

`cargo build --features tauri/custom-protocol` turns dev mode off. `tauri-codegen`
then requires `frontendDist` (`../dist`) and embeds icons. Missing dist, a broken
`tauri.conf.json`, or a missing icon fails that build. `pnpm tauri build` also does
this, and then bundles a DMG or MSI and runs `beforeBuildCommand`. That is the wrong
weight for every pull request.

The `rust` job sets `defaults.run.working-directory` to `src-tauri` (`.github/workflows/ci.yml`).
A new `run` step inherits that. `pnpm install` and `pnpm build` must set
`working-directory: .` or they look for `package.json` inside `src-tauri` and fail.
`cargo build` must set no `working-directory` of its own; the job default is already
the crate. `uses` steps (setup-node, the pnpm store cache) ignore that default.

The production CSP is applied when that embedded HTML is served
(`tauri` `AppManager` calls `set_csp` on `.html` assets). It is not written into
`dist/index.html` by `pnpm build`. Grepping `dist/` for a CSP meta tag will not work.

### 2. A10–A12 already have tests. A13's policy can be checked statically; enforcement cannot

| ID | Where | Gap |
|---|---|---|
| A10 | `checksums_and_frames_are_verified` in `src-tauri/src/library/tests.rs` flips one byte of `pages/0004.png` and of `originals/0001.bmp` and expects `ChecksumMismatch` for that path. Runs on every OS. | None. Do not add a test. |
| A11 | `destination_rules` creates a directory `Other-Book` and expects `DestinationExists` for id `other-book`. `id_taken` compares lowercased names, so the test is real on Linux, not only on a case-insensitive volume. | None. Do not add a test. |
| A12 | `resolve_inside` in `src-tauri/src/library/load.rs` canonicalizes `package_root.join(file)` and rejects a path that does not `starts_with` the canonical root. `page_paths_stay_inside_the_package` (`#[cfg(unix)]`) swaps `pages/0000.jpg` for a symlink to a file outside the book and expects the "outside" error. Originals use the same `verify_file`. | No Windows symlink test. Creating one on GitHub runners is unreliable, and a skipped test would not prove the prefix. `book_view_paths_are_absolute_canonical_and_under_pages` already runs on Windows and checks absolute canonical paths under `pages/`. Accept the gap. |
| A13 | `src-tauri/tauri.conf.json` `app.security.csp` is an object, not `null`. Production `script-src` is `'self'` only. | Add a pytest that locks that object, `assetProtocol`, and `capabilities/default.json`. Do not claim the webview enforces it. Section 8 of the manual script stays the runtime check. |

### 3. Asset-protocol checklist: eight items hold, and none need a code change

Checked against `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`,
`src-tauri/src/commands/mod.rs` (`open_and_register`), `src-tauri/src/commands/view.rs`
(`book_view`), `src-tauri/src/library/load.rs` (`resolve_inside`), `src/lib/api.ts`
(`pageUrl`), and Tauri 2.12's scope and `convert_file_src` (Windows and Android use
`http://asset.localhost` unless `useHttpsScheme` is set; this app does not set it).

| Checklist item | Verdict | Code change |
|---|---|---|
| `assetProtocol.enable: true` | Holds (`tauri.conf.json`). | None |
| Explicit CSP, not `null` | Holds. Production and `devCsp` are both objects. | None. Static lock is chunk 2. |
| Static `assetProtocol.scope` is `[]`; runtime admits only verified page files | Holds. `open_and_register` calls `allow_file` only for `BookView` page paths from `load_book`. `originals/` are not admitted. The capability set is `core:default` and `dialog:allow-open`. No `fs` plugin. | None |
| CSP `img-src` and `connect-src` allow `asset:` and `https://asset.localhost` | Holds. Both directives also list `http://asset.localhost` (the Windows default). `connect-src` is required because the decode worker `fetch`es the page. No `media-src` (no audio or video). | None |
| `convertFileSrc` receives absolute paths only | Holds. `PageView.path` is the canonical path from `LoadedBook::page_path`, passed through `dunce::simplified` (a no-op on macOS and Linux). `book_view_paths_are_absolute_canonical_and_under_pages` asserts absolute, under `pages/`, and stable across `canonicalize`. | None |
| Canonicalize after `join` and require the path to stay under the package (A12) | Holds. See the A12 row above. | None |
| macOS `/var` → `/private/var` | Holds without a special case. `canonicalize` resolves the symlink before the path is stored on the view and passed to `allow_file`. Tauri's scope also stores `canonicalize_parent` of the allowed path and canonicalizes again in `is_allowed`. The app-data library is under `~/Library/Application Support`, not `/var`. The same rule covers `/tmp` (a symlink to `/private/tmp`). | None. Do not string-replace `/var`. |
| Windows `https://asset.localhost` | Config allows it. The URL this app will actually request on Windows is `http://asset.localhost`. Runtime confirmation stays the manual WebView2 item on the follow-ups plan. | None. Do not set `useHttpsScheme`. |
| Persisted-scope plugin not required | Holds. Nothing in `Cargo.toml` or the capability file uses it. Scope is rebuilt with `allow_file` on each open. Revisit only if a user-chosen library root ships. | None |

The Phase 2 changelog sentence that says scope is `$APPDATA/books/*/pages/**` describes
an older config. The shipped scope is empty. Do not edit that released changelog entry.

### 4. `pageLabel` gets `maxLength` 256 (LC-T11)

Add `"maxLength": 256` on `pageLabel` in both
`docs/book-format.v1.schema.json` and `docs/book-format.v2.schema.json`, and the same
limit in `PageEntry::validate` using `chars().count()` (Unicode scalars, the same count
`title` already uses and the same count JSON Schema `maxLength` uses with the
`jsonschema` 0.51 crate).

256, not 4096 or 8192. The field is an optional short label (the Rust docs say a printed
page number). The reader does not show it. A book may have 100,000 pages; a title-sized
label on every page is tens or hundreds of megabytes and only fails later, as the 64 MB
`meta.json` cap, with a worse error. 256 scalars is enough for a plate caption. An empty
string stays valid. `None` stays omitted on write.

Do not bump `formatVersion`. This tightens an existing field. The app writes
`page_label: None` (`create.rs`). The fixture generator writes `"Cover"`. A hand-edited
package whose label is longer than 256 characters will start failing schema validation
and `BookMeta::validate`. Say that in `CHANGELOG.md`.

Reuse `BookMetaError::TextFieldTooLong { field: "pageLabel", .. }`. Do not add a variant.
The variant's doc comment currently says the field is `rights` or `attribution`. Change
it so it names `pageLabel` too. Do not add a TypeScript constant: no form edits this
field (`src/lib/limits.ts` stays tied to title, rights, and attribution).

Put `PAGE_LABEL_MAX_LEN = 256` in `scripts/generate-fixture-book.py` next to
`TITLE_MAX_LEN`. The generator's only label is the literal `"Cover"`, which is under
the cap, and it has no label flag. The constant exists so
`tests/test_generate_fixture_book.py` can lock it to the schema the way it already
locks `TITLE_MAX_LEN`, `TEXT_FIELD_MAX_LEN`, and `PAGES_MAX_ITEMS`. Assert it against
`pageLabel.maxLength` in both v1 and v2 (`$defs.pageEntry.properties.pageLabel`).

### 5. `jsonschema` does not need its default features

`jsonschema` 0.51.0 defaults are `resolve-http`, `resolve-file`, and `tls-aws-lc-rs`.
`resolve-http` pulls `reqwest` and `rustls`; `tls-aws-lc-rs` pulls `aws-lc-sys`.
On the host graph today those crates appear only under `jsonschema`:

```text
cargo tree -i reqwest -e normal,build
cargo tree -i aws-lc-sys -e normal,build
cargo tree -i rustls -e normal,build
```

each ends at `scriptorium` via `jsonschema`. Tauri's own `reqwest` dependency is
limited to Android and non-macOS Apple targets. This app does not enable `rustls-tls`
or `native-tls`.

The loader and the schema tests call `jsonschema::options().should_validate_formats(true).build`
and `jsonschema::validator_for`. Formats such as `date-time`, and `$ref` to `#/$defs/...`,
are built in. They do not use `resolve-http`, `resolve-file`, `resolve-async`, or a TLS
feature. The embedded schemas' `$schema` URL selects a built-in draft; unit tests already
compile those schemas without a network fetch.

Set `default-features = false` and enable no features. If a schema test then fails
because a `https://` URI could not be retrieved, stop. Do not turn `resolve-http` back
on. The schemas are embedded with `include_str!` and must keep compiling offline.

`resolve-file` adds no crates, and it is still the wrong feature: it allows the
validator to read `file://` URLs. The book schemas never do that.

## Approach

Five code commits after a short backlog registration, each revertable on its own.
Chunks 2 and 3 both edit the follow-ups plan, so land them in order. Chunks 1, 4, and
5 touch different files and can be reviewed separately.

Do not mass-tick the M1 section of the roadmap. Those boxes are stale relative to
0.2.0. Tick only the M0.5 line that is specifically LC-M2.

### Alternatives considered

| Option | Why not chosen |
|---|---|
| Plain `cargo build` with no extra feature | Does not embed `dist/` on Tauri 2.12, so it does not catch LC-M2. |
| `pnpm tauri build` in CI | Also bundles installers and reruns the licence check. Too slow for the fast lane. |
| Windows and macOS matrix for the embed build | The missing-dist and bad-config failure is the same on Linux. macOS minutes are costly, and the manual Windows run stays on the follow-ups plan. |
| Grep `dist/index.html` for a CSP meta tag | Tauri injects the policy when it serves the embedded HTML, not during `pnpm build`. |
| Tick the archived checklist in place | That plan is historical. The verdict lives here, and the follow-ups checkbox points here. |
| `pageLabel` `maxLength` 4096, or no limit | 4096 matches `title` in name only. No limit leaves a 100,000-page document unbounded until the 64 MB cap. |
| `formatVersion` 3 for the label cap | The field already exists. v1 and v2 gain the same constraint. |
| Keep `resolve-file`, or switch TLS to `tls-ring` | Nothing resolves external schemas. Ring would still link a TLS stack the app does not use. |
| Leave AWS-LC in `ci/bundled-c.toml` after the crates drop | The NVD job fails closed when a listed crate is missing from the lockfile. |

## Proposed file changes

```text
to_do.md                         — chunk 0 registers this plan; chunk 5 drops the jsonschema icebox line
plans/README.md                  — chunk 0 adds the active-plan row
plans/2026-11-02-m1-followups.md — chunks 1–3 tick or split the items this work closes
plans/2026-08-23-product-roadmap.md — chunk 1 ticks only the M0.5 LC-M2 line
.github/workflows/ci.yml         — chunk 1: pnpm build, then cargo build --features tauri/custom-protocol
.github/workflows/security.yml   — chunk 5: comment no longer lists AWS-LC
docs/ci-and-hooks.md             — chunk 1: the Rust job also embeds the frontend
tests/test_tauri_csp.py          — chunk 2: static CSP, asset protocol, capability set
docs/book-format.v1.schema.json  — chunk 4: pageLabel maxLength 256
docs/book-format.v2.schema.json  — chunk 4: same
docs/book-format.md              — chunk 4: the field note and the loader table
src-tauri/src/book_format/mod.rs — chunk 4: PAGE_LABEL_MAX_LEN, PageEntry::validate, TextFieldTooLong docs
src-tauri/src/book_format/tests_v2.rs — chunk 4: both schemas and the Rust bound
scripts/generate-fixture-book.py — chunk 4: PAGE_LABEL_MAX_LEN = 256
tests/test_generate_fixture_book.py — chunk 4: lockstep assert against both schemas
src-tauri/Cargo.toml             — chunk 5: jsonschema default-features = false
src-tauri/Cargo.lock             — chunk 5: dropped HTTP and AWS-LC crates
ci/bundled-c.toml                — chunk 5: remove the AWS-LC library
THIRD_PARTY_NOTICES.md           — chunk 5: remove the AWS-LC section and its advisory bullet
THIRD_PARTY_LICENSES.generated.md — chunk 5: regenerated
CHANGELOG.md                     — chunks 4 and 5
CHANGELOG.dev.md                 — chunks 1, 2, and 5
```

## Phases & checklist

Land the chunks in order. Mark a box only when that chunk's verification commands have
passed. Commit a chunk only after `bash scripts/sonar_scan.sh` reports no new issues.
`ACTION REQUIRED` must not list anything that chunk added. Fix those issues and scan
again before the commit. Do not set `SONAR_FRESHNESS_GATE=0` and do not pass
`--no-verify`.

`library/tests.rs` is 733 lines and `book_format/tests.rs` is 573. Do not add
tests to either. `book_format/mod.rs` is 564; the label check belongs in
`PageEntry::validate`, which is short, not in `BookMeta::validate`.

### Chunk 0: Register the plan

Files: `to_do.md`, `plans/README.md`.

- [x] In **Active plans**, add a row for this file, status `draft` (then `in-progress`
      when chunk 1 starts).
- [x] In **Next Up**, add this plan as a pointer (the section stays at 3–5 lines).
      Reword the existing follow-ups pointer so it is the manual remainder (script,
      Windows, A9, ADR open items), not a second copy of LC-M2 and A10–A13.
- [x] Add this file to the active-set table in `plans/README.md`.
- [x] No changelog entry.

### Chunk 1: Debug `cargo build` in CI (LC-M2)

Files: `.github/workflows/ci.yml`, `docs/ci-and-hooks.md`,
`plans/2026-11-02-m1-followups.md`, `plans/2026-08-23-product-roadmap.md`,
`CHANGELOG.dev.md`.

Steps:

- [x] On the existing `rust` job only (ubuntu, after `cargo test` and `cargo check`),
      add Node the same way `typecheck-ts` does. Pin every new `uses` step to a full
      SHA plus a version comment, copying the SHAs already in `ci.yml`. Do not add a
      new action and do not use a floating tag. The pins are
      `actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1` (`# v7.1.0`) and
      `actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` (`# v6.1.0`).
      `node-version: 24`. Cache `~/.local/share/pnpm/store` with key
      `pnpm-${{ runner.os }}-${{ hashFiles('pnpm-lock.yaml') }}`.
- [x] `run` steps inherit `working-directory: src-tauri`. Set `working-directory: .`
      on `corepack enable`, `pnpm install --frozen-lockfile`, and `pnpm build`.
      Do not set `working-directory` on `cargo build --features tauri/custom-protocol`.
      Do not pass `--release`. Do not run `pnpm tauri build`.
- [x] Leave `cargo fmt`, `clippy`, `test`, and `check` without that feature, so they
      keep passing when `dist/` is absent. Do not add the step to `rust-cross`.
- [x] Document the step in the Rust row of `docs/ci-and-hooks.md`, including the
      `tauri/custom-protocol` reason.
- [x] Tick the follow-ups line "Add a debug `cargo build` to CI (LC-M2)".
- [x] Tick only the roadmap M0.5 line "Add `cargo build` to CI Rust job (**LC-M2**)".
      Leave the M1 "Close remaining M0 review…" box alone.
- [x] `CHANGELOG.dev.md` `## [Unreleased]`: the Rust CI job embeds the production
      frontend with `cargo build --features tauri/custom-protocol` after `pnpm build`.

Tests: none. The job is the test.

Verification:

- [x] Locally, with `dist/` absent, `cargo build --features tauri/custom-protocol` in
      `src-tauri` fails because `frontendDist` does not exist. Restore `dist/` with
      `pnpm build` and the same command succeeds. Do not commit `dist/`.
- [x] `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` in
      `src-tauri` still pass without the feature.
- [x] PR note for the workflow change, per [`policies/github-actions-usage.md`](../policies/github-actions-usage.md):
      same `pull_request` and `push` to `main` triggers; one extra Linux leg on the
      existing `rust` job; about 5–12 minutes on a warm cargo cache (a feature-flipped
      Tauri compile plus `pnpm build`); no new artifact; public-repo minutes are not
      billed. No macOS or Windows runner added.

### Chunk 2: A10–A12 confirmation and a static A13 test

Files: `tests/test_tauri_csp.py` (new), `plans/2026-11-02-m1-followups.md`,
`CHANGELOG.dev.md`.

Steps:

- [x] Do not add Rust tests for A10, A11, or A12. The findings table is the confirmation.
- [x] Split the follow-ups hardening bullet into three:
      - A10–A12 have the tests named above. Tick it.
      - A static test locks the production CSP and the empty asset scope. Tick it when
        the pytest is in.
      - A release build still has to be opened by hand (manual script section 8, and
        the Windows WebView2 bullet). Leave that unchecked.
- [x] Add `tests/test_tauri_csp.py`. Stdlib `json` and `pathlib` only. Read
      `src-tauri/tauri.conf.json` and `src-tauri/capabilities/default.json`. Assert all
      of the following, and fail with the field name when one drifts:
      - `app.security.csp` is an object. It is not `null` and not a string.
      - `csp` `default-src` is `["'self'"]`.
      - `csp` `script-src` is `["'self'"]` (no `'unsafe-inline'`, no `'unsafe-eval'`).
      - `csp` `img-src` contains `'self'`, `asset:`, `http://asset.localhost`,
        `https://asset.localhost`, `data:`, and `blob:`.
      - `csp` `connect-src` contains `'self'`, `asset:`, `http://asset.localhost`,
        `https://asset.localhost`, `ipc:`, and `http://ipc.localhost`.
      - `csp` `object-src` is `["'none'"]` and `base-uri` is `["'none'"]`.
      - `csp` `style-src` is `["'self'", "'unsafe-inline'"]` (lock the current policy;
        do not tighten it in this chunk).
      - `devCsp` is present, and its `script-src` contains `'unsafe-inline'` and does
        not contain `'unsafe-eval'`. `devCsp` does not replace `csp`.
      - `app.security.assetProtocol.enable` is true and `scope` is `[]`.
      - `capabilities/default.json` `permissions` is exactly `core:default` and
        `dialog:allow-open`.
- [x] `CHANGELOG.dev.md` `## [Unreleased]`: A10–A12 were already covered; a pytest now
      locks the CSP object. The release-build console check is still manual.

Verification:

- [x] From the repo root, with the project venv if one exists:
      `python -m pytest tests/test_tauri_csp.py -q`
- [x] Temporarily set `csp` to `null` in a throwaway edit, confirm the test fails, and
      restore the file before committing. Do not leave that edit in the diff.
- [x] `ruff check tests/test_tauri_csp.py` and `ruff format --check tests/test_tauri_csp.py`.
- [x] `basedpyright` (same as the `Type check (Python)` job; `pyrightconfig.json`
      `include` already covers `tests/`, so the new file is in that run).

### Chunk 3: Record the asset-protocol verdict

Files: `plans/2026-11-02-m1-followups.md` only.

Steps:

- [ ] Tick "Verify the asset-protocol checklist … and tick each item that holds."
      In the same bullet, point at the findings table in this plan. State that eight
      items hold with no code change, and that the Windows `https://asset.localhost`
      runtime check remains the existing manual WebView2 bullet (do not tick that
      bullet).
- [ ] Do not edit the archived M1 plan.
- [ ] No changelog entry. The verdict is this plan plus the ticked follow-up.

Verification:

- [ ] `python ci/scripts/check_doc_links.py --internal-only --strict plans/2026-11-02-m1-followups.md plans/2026-11-05-m1-hardening.md`
      Pass those paths. With no paths the script `rglob`s the repo, and `SKIP_DIRS`
      does not include `.venv` or `.context`, so a bare run walks both. The new
      pointer must resolve.

### Chunk 4: `pageLabel` maxLength 256 (LC-T11)

Files: both book-format schemas, `docs/book-format.md`,
`src-tauri/src/book_format/mod.rs`, `src-tauri/src/book_format/tests_v2.rs`,
`scripts/generate-fixture-book.py`, `tests/test_generate_fixture_book.py`,
`CHANGELOG.md`, `CHANGELOG.dev.md`. Tick the follow-ups `pageLabel` bullet in
`plans/2026-11-02-m1-followups.md`.

Steps:

- [ ] In both schemas, set `pageLabel` to `type: string`, `maxLength: 256`, and a
      description that says the cap is 256 Unicode characters. Leave it optional.
      Do not add `minLength`.
- [ ] Add `pub const PAGE_LABEL_MAX_LEN: usize = 256` next to `TITLE_MAX_LEN`, with a
      doc comment that it matches both schemas and is a Unicode scalar count.
- [ ] In `PageEntry::validate`, if `page_label` is `Some` and
      `chars().count()` exceeds `PAGE_LABEL_MAX_LEN`, return
      `TextFieldTooLong { field: "pageLabel", len, max: PAGE_LABEL_MAX_LEN }`.
- [ ] Update the `TextFieldTooLong` doc comment so it names `pageLabel` as well as
      `rights` and `attribution`.
- [ ] In `docs/book-format.md`, note the cap on the `pageLabel` row and add a loader-table
      row: `pages[].pageLabel` must be at most 256 characters when present.
- [ ] In `tests_v2.rs` (279 lines; keep the new test short):
      - Both schemas' `pageLabel.maxLength` equals `PAGE_LABEL_MAX_LEN`.
      - A label of 256 `"é"` scalars passes `BookMeta::validate` and both schema
        validators. 257 fails both. `"é"` is one scalar and two UTF-8 bytes; if the
        schema and Rust disagree, stop and report it. Do not switch the count to bytes.
      - The existing `"Cover"` round-trip in `tests.rs` stays as it is.
- [ ] In `scripts/generate-fixture-book.py`, add `PAGE_LABEL_MAX_LEN = 256` next to
      `TITLE_MAX_LEN`, under the same lockstep comment. Leave the written label as
      `"Cover"`. Do not add a CLI flag.
- [ ] In `tests/test_generate_fixture_book.py` `test_book_id_pattern_matches_schema`,
      assert `PAGE_LABEL_MAX_LEN` equals `pageLabel.maxLength` on both
      `docs/book-format.v1.schema.json` and `docs/book-format.v2.schema.json`
      (`$defs.pageEntry.properties.pageLabel`). That test currently reads only v2;
      load v1 for this assert as well.
- [ ] `CHANGELOG.md` `## [Unreleased]` `### Changed`: a page label may be at most 256
      characters; a longer one is rejected.
- [ ] `CHANGELOG.dev.md` `## [Unreleased]`: `PAGE_LABEL_MAX_LEN` is locked to both
      schemas in `tests_v2.rs` and in `tests/test_generate_fixture_book.py`.
- [ ] Tick the follow-ups `pageLabel` bullet, and name 256 and the "no formatVersion
      bump" reason in that bullet.

Verification:

- [ ] `cargo fmt --check` and `cargo test --manifest-path src-tauri/Cargo.toml`
      (the schema and validate tests, including the new one).
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
      `tests_v2.rs` is `#[cfg(test)]`. `cargo clippy` without `--all-targets` does not
      lint it. `rust-cross` already passes `--all-targets`; this local command matches
      that.
- [ ] `python -m pytest tests/test_generate_fixture_book.py -q` (the new lockstep
      assert). `ruff check` on the two Python files this chunk edits.
- [ ] Public item doc comments stay in place (`missing_docs`), including the updated
      `TextFieldTooLong` comment. The new branch in `PageEntry::validate` stays under
      the cognitive complexity limit of 15.

### Chunk 5: Drop `jsonschema`'s network stack

Files: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `ci/bundled-c.toml`,
`.github/workflows/security.yml` (comment only), `THIRD_PARTY_NOTICES.md`,
`THIRD_PARTY_LICENSES.generated.md`, `CHANGELOG.md`, `CHANGELOG.dev.md`, `to_do.md`.
Tick nothing on the follow-ups plan; this item lived under **Unplanned / small**.

Steps:

- [ ] Change the dependency to
      `jsonschema = { version = "0.51.0", default-features = false }`
      with no `features` list.
- [ ] Let Cargo rewrite `Cargo.lock` by building or testing. Do not hand-edit the lockfile.
- [ ] Confirm the host graph no longer contains `reqwest`, `rustls`, `aws-lc-rs`, or
      `aws-lc-sys` (`cargo tree -i <crate> -e normal,build` from `src-tauri` exits
      because the package is absent). If any of them still has a parent other than
      `jsonschema`, stop and report that parent. Do not keep `tls-ring` as a substitute.
- [ ] Delete the `## AWS-LC` section of `THIRD_PARTY_NOTICES.md` (heading, bullets, and
      the fenced licence text) and the AWS-LC bullet under `## Advisory check`.
- [ ] Delete the AWS-LC `[[library]]` from `ci/bundled-c.toml`. Update the
      `bundled-c-cves` comment in `.github/workflows/security.yml` so it names OpenJPEG
      and Little CMS only.
- [ ] Regenerate notices: `pnpm licenses:gen`, then `pnpm licenses:check`. Commit the
      generated licence file. `about.toml` does not need a new accepted licence; crates
      are leaving, not arriving.
- [ ] Remove the **Trim jsonschema's network stack** bullet from `to_do.md`
      **Unplanned / small**.
- [ ] `CHANGELOG.md` `## [Unreleased]` `### Security`: the app no longer links an HTTP
      client or AWS-LC. Book schemas are compiled from files embedded in the binary.
- [ ] `CHANGELOG.dev.md` `## [Unreleased]`: `jsonschema` is built with
      `default-features = false`; the bundled-C manifest and the generated licence file
      dropped the AWS-LC crates.

Verification:

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (schema compile and `date-time`
      validation still pass with no network).
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`.
- [ ] The four `cargo tree -i` checks above.
- [ ] `pnpm licenses:check`.
- [ ] `cargo audit` in `src-tauri` (expect exit 0; do not add an ignore).
- [ ] `python3 ci/scripts/check_bundled_c_cves.py` still runs for OpenJPEG and Little CMS.
      It must not look up `aws-lc-sys`.

## Verification

- [ ] Each chunk's commands above passed, and the chunk's boxes are ticked only after that.
- [ ] Each chunk was committed only after its own `bash scripts/sonar_scan.sh` reported
      no new issues. A scan at the end of the branch does not replace the per-commit scan.
- [ ] `CHANGELOG.md` has Unreleased notes for the label cap and the dropped HTTP stack.
      `CHANGELOG.dev.md` has Unreleased notes for the CI embed, the CSP pytest, and the
      `jsonschema` feature change. No `VERSION` bump.
- [ ] Follow-ups still lists the manual script, the Windows release build, A9, and the
      ADR open items as open. This plan does not close that file.

## Open questions

None. The label cap, the CI feature flag, the A12 Windows gap, and the empty
`jsonschema` feature list are decided above. A reviewer who wants a different cap
should say so before chunk 4.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `custom-protocol` rebuild adds several minutes to the Rust job | high | low | One Linux job, no matrix, no artifact. Noted in the PR. |
| Disabling `jsonschema` defaults breaks schema compile because `$schema` is fetched | low | med | Chunk 5 says stop and do not re-enable HTTP. Embedded drafts are built in; existing tests already compile offline. |
| `cargo about` still lists AWS-LC because a desktop crate other than `jsonschema` needs it | low | med | The tree check is the gate. If a real parent remains, keep the notices and the bundled-C entry for that parent. |
| A 256-character cap rejects a hand-written label someone already depends on | low | low | The app never writes a label. The changelog says so. No formatVersion bump. |
| Static CSP test is mistaken for webview enforcement | med | med | Chunk 2 leaves the manual release-build bullet open. |

## Completion steps (when status = complete)

When this plan is complete, follow this workflow:

1. **Update plan status** to `complete`.
2. **Move this file** to `plans/archive/completed/`. See [`README.md`](README.md).
   In the same change, rewrite the link this plan adds in
   `plans/2026-11-02-m1-followups.md`. That file sits in `plans/`, so a same-directory
   link to `2026-11-05-m1-hardening.md` 404s once this file moves. Point it at
   `archive/completed/2026-11-05-m1-hardening.md`. Then run
   `python ci/scripts/check_doc_links.py --internal-only --strict plans/2026-11-02-m1-followups.md plans/archive/completed/2026-11-05-m1-hardening.md`
   (pass the paths; a bare run also walks `.venv` and `.context`).
3. **Log every meaningful completion once.** Chunks 1, 2, 4, and 5 already add the
   Unreleased lines. Do not duplicate them at archive time. Public impact is the label
   cap and the dropped HTTP stack (`CHANGELOG.md`). CI, the pytest, and the licence
   regeneration are `CHANGELOG.dev.md`.
4. **Remove this plan from `to_do.md`** (**Next Up** and **Active plans**). Leave the
   follow-ups plan listed while its manual items are open.
5. **Clean up `.context/` scratch** if a session wrote any.
6. **Update `orchestration-state.md`** if present.
7. **Reference the completion** with the PR that landed the chunks.

Use [`prompts/cleanup-completed-work.md`](../prompts/cleanup-completed-work.md) for the
archive pass. The follow-ups plan is not finished by this one.
