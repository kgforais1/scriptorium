# Plan: M2a Phase 3, the Tesseract engine and bundling

Last reviewed: 2026-11-23
Status: complete (merged in #86, #93, #96 and #98)

This is the implementation plan for Phase 3 of
[`2026-11-07-m2a-ocr-pipeline.md`](../../2026-11-07-m2a-ocr-pipeline.md). The parent plan's
"Engine interface" and "Bundling Tesseract" sections hold the design. This plan fixes the
order of work, the details those sections leave open, and the checks for each chunk. Where the
two disagree, this plan wins for Phase 3, and the parent plan is corrected in the same PR.

## Goal

The app can run the bundled Tesseract on one image and return a `PageOcr`. A fresh clone gets the
binary and language data with one fetch script. CI proves the fetched binary works on macOS,
Windows, and Linux, and that a built bundle runs its own sidecar. No job, command, or UI is added.
Those are Phase 4.

## Decisions this plan adds

1. **Release asset naming.** The first release tag was `tesseract-5.5.3-r1`. `tesseract-5.5.3-r2` rebuilt the Windows binary with a UTF-8 code page manifest (#92), and is the one pinned. The `r` number goes up when
   the binaries are rebuilt with the same Tesseract version, for example after a library bump.
   Each asset is the bare binary, named `tesseract-<target-triple>[.exe]`. It is installed as
   `src-tauri/binaries/scriptorium-tesseract-<target-triple>[.exe]`, because a sidecar named
   `tesseract` would clash with a distro `tesseract-ocr` package's `/usr/bin/tesseract` in
   Tauri's `.deb`; the release assets keep their own names. The assets are:
   `tesseract-aarch64-apple-darwin`, `tesseract-x86_64-unknown-linux-gnu`, and
   `tesseract-x86_64-pc-windows-msvc.exe`. There is no x86_64 macOS asset. The app's macOS target is
   arm64 only today, and `fetch_tesseract.sh` exits with a clear message on an Intel Mac.
2. **Publishing is manual.** `tesseract.yml` gains a `publish` job. It runs only on
   `workflow_dispatch` with a `release_tag` input, and is the only job with `contents: write`. An
   `assets` job, which runs on every run, gathers the three binaries and writes `SHA256SUMS`. The
   publish job takes that release layout and creates the release with `gh release create`. Builds are not bit-for-bit reproducible, so the hashes pinned
   in `fetch_tesseract.sh` are the hashes of the published assets, copied from `SHA256SUMS`, and
   checked by downloading each asset once by hand. Dispatching the publish run creates a public
   release, so it needs the user's approval each time.
3. **Language data comes from upstream, not the release.** `fetch_tesseract.sh` downloads
   `eng.traineddata` from `tesseract-ocr/tessdata_fast` at the pinned tag `4.1.0`, with the hash
   the Phase 1 smoke step already checks. It installs it at `src-tauri/tessdata/eng.traineddata`.
4. **Every Rust build needs the sidecar file.** `tauri_build::build()` fails when a file named in
   `bundle.externalBin` is missing for the target triple. That covers `cargo build`, `cargo test`,
   and `cargo clippy`. So the fetch lands in every Rust CI job (`rust`, `rust-coverage`,
   `rust-cross`, and any other job that compiles `src-tauri`) in Chunk 2, before Chunk 5 adds
   `externalBin`. In Chunk 5, `beforeDevCommand` and `beforeBuildCommand` gain `pnpm tesseract:fetch`.
   `docs/development.md` says a fresh clone runs `pnpm tesseract:fetch` once before `cargo test`.
5. **The override variable is debug only.** `SCRIPTORIUM_TESSERACT` and
   `SCRIPTORIUM_TESSDATA` are read only under `cfg(debug_assertions)`. A release build never takes
   a binary path from the environment.
6. **The compile-time target triple.** `build.rs` exports `TARGET` as
   `SCRIPTORIUM_TARGET_TRIPLE` with `cargo:rustc-env`. The debug fallback looks for
   `src-tauri/binaries/scriptorium-tesseract-<that triple>[.exe]`.
7. **Paths reach Tesseract as relative names.** The child runs with its working directory set to
   the image's directory. The image and the output base are passed as bare file names. This keeps
   a non-ASCII user name in the temp path away from Tesseract's argument handling on Windows. The
   `--tessdata-dir` path is still absolute, so the Windows integration test runs from a directory
   with a non-ASCII name and checks that the data loads (see Chunk 4). If it fails, the fix is to
   copy the language data into the run directory and pass it relatively too.
8. **No console window on Windows.** The child is spawned with `CREATE_NO_WINDOW`
   (`0x0800_0000`) through `std::os::windows::process::CommandExt::creation_flags`. Without it
   every page would flash a console window in a release build.
9. **The cleared environment.** The child starts from `env_clear()`. It then gets
   `TESSDATA_PREFIX`, `OMP_THREAD_LIMIT=1`, and, on Windows only, `SystemRoot`, which some Windows
   APIs need to start. Nothing else is passed.
10. **Paragraph numbers.** TSV `par_num` restarts in each block, so `(block_num, par_num)` is the
    paragraph key. The parser numbers paragraphs from 1 in order of first appearance and stores
    that number in `Line.paragraph`.
11. **CVE tracking.** `check_bundled_c_cves.py` reads versions from crate sources only. It gains a
    second source kind, `source = "script"`, which reads the version with a regex from a named
    file, here `scripts/build_tesseract.sh`. Tesseract, Leptonica, zlib, libpng, libjpeg-turbo,
    libtiff, and libwebp then go in `ci/bundled-c.toml`, each with a verified CPE and canary. Any
    library with no usable canary is listed in the bump policy for a manual check instead.

## Chunks

Each chunk is one commit. The implementer writes it. Claude and the reviewer both review it, and
`bash scripts/sonar_scan.sh` must report "No new issues (was 22)." before it is committed.

### Chunk 1: publish the binaries

- `tesseract.yml`: the Windows build sets `TESSERACT_TARGET_TRIPLE=x86_64-pc-windows-msvc`. Each
  build job checks its output's name matches the expected asset name.
- On Windows, a separate step with `shell: msys2 {0}` checks the `.exe` imports no MinGW
  runtime DLL (`libgcc_s_seh-1.dll`, `libstdc++-6.dll`, `libwinpthread-1.dll`), with the MSYS2
  `objdump -p`. The job's default shell is Git Bash, whose `objdump` may be missing.
- Add an `assets` job with `needs: [build]`, which runs on every run. The matrix `build` job
  uploads one artifact per runner, named `tesseract-<os>`. The `assets` job downloads all three,
  checks each holds exactly the expected `tesseract-<triple>[.exe]`, writes `SHA256SUMS`, and
  uploads the result as the `release` artifact.
- Add the `publish` job from decision 2, with `needs: [assets]`. It runs only from the default
  branch, refuses an existing release or tag, sets `concurrency` so two publishes cannot race,
  and creates a draft outside "latest" that it publishes once every asset is uploaded.
- Add `workflow_dispatch` input `release_tag`, with no default.
- Check: the workflow runs green on the PR. Once the PR merges, the user approves, and the publish run is
  dispatched on `main`, so the tag points at a merged commit. The three asset hashes are recorded from `SHA256SUMS` and from a local download.

### Chunk 2: fetch script and CI wiring

- `scripts/fetch_tesseract.sh`, modelled on `fetch_pdfium.sh`: the same curl flags, `sha256_of`,
  and platform detection. It installs the host's binary into `src-tauri/binaries/`, makes it
  executable, and installs `eng.traineddata` into `src-tauri/tessdata/`. It writes
  `.version` files holding the release tag and platform, and the tessdata tag, and exits early
  when both match. `TESSERACT_ASSET_CACHE` keeps downloads between runs, as
  `PDFIUM_ARCHIVE_CACHE` does. On a hash mismatch it deletes the cached file before it exits,
  so a corrupt download cannot fail every later run.
- `package.json`: `"tesseract:fetch": "bash scripts/fetch_tesseract.sh"`.
- `.gitignore`: `src-tauri/tessdata/` and `.tesseract-assets/`.
- `ci.yml`: every job that compiles `src-tauri` caches `.tesseract-assets` keyed on the script's
  hash, and runs the fetch next to the pdfium fetch.
- Check: run the script twice locally. The second run exits early. A corrupted cached file fails
  the hash check and is not installed.

### Chunk 3: `ocr/tesseract.rs`

- `pub struct TesseractEngine { binary: PathBuf, tessdata: PathBuf }` with `fn locate(resource_dir:
  Option<&Path>) -> Result<Self, OcrUnavailable>`. The order is: the debug override variables,
  then the directory of `std::env::current_exe()` (`tesseract[.exe]`, the name Tauri gives the
  sidecar), then, in debug builds only, `src-tauri/binaries/tesseract-<triple>[.exe]`. The
  language data is `<resource_dir>/tessdata`, then, in debug builds, `src-tauri/tessdata`.
  `locate` checks both files exist. It does not run the binary.
- `id()` returns `tesseract 5.5.3 eng-fast` from constants that `fetch_tesseract.sh`'s pin
  matches. A unit test reads the script and checks the two agree.
- `recognize`: refuse empty `langs` before anything else. Spawn as decisions 7 to 9 and the
  parent plan describe. Stdout goes to null. Stderr goes to a file in the image's directory. Poll
  `try_wait` every 100 ms. On `stop` return `Cancelled`, and past `deadline` return `Timeout`,
  after `kill()` and `wait()` in both cases. On a non-zero exit return `Engine` with the last 2 KB
  of stderr. Then parse `<base>.tsv`. Remove the TSV and stderr files before returning, on every
  path.
- `parse_tsv(&str) -> Result<PageOcr, OcrError>`, a pure function. The header row must match the
  12 expected column names. The level 1 row gives width and height. Level 4 rows start lines.
  Level 5 rows are words. A word with empty trimmed text or a confidence below 0 is dropped.
  Confidence is divided by 100 and clamped. A row with the wrong column count, or a number that
  will not parse, is an `Engine` error naming the row. A TSV with no level 1 row is an error. A
  page with no words is a valid empty `PageOcr`.
- Unit tests on recorded TSV in `src-tauri/src/ocr/testdata/`: a normal page, a page with empty
  rows and `-1` confidences, a multi-block page whose `par_num` restarts, an empty page (header
  and page row only), a truncated row, and a CRLF file. The recorded files come from the fetched
  binary on the fixture pages, so they are real output.
- Spawn tests that need no real Tesseract use a tiny stand-in program, built by Cargo as a test
  binary under `src-tauri/tests/bin/` or `examples/`. It can sleep, exit non-zero, or write a fixed
  TSV, depending on its arguments. They cover cancel within a second, timeout, a non-zero exit with
  stderr in the error, and cleanup of the temp files on each path.

### Chunk 4: the integration test

- `src-tauri/tests/tesseract_engine.rs` runs `TesseractEngine::locate` and `recognize` on
  `tests/fixtures/ocr/smoke.png` and checks it finds "PRINCESS" with a box inside the page.
- First, it asserts no `tesseract` is on `PATH`.
- When the binary is missing, the test fails if `SCRIPTORIUM_REQUIRE_TESSERACT=1`, and is skipped
  with a printed note otherwise. CI sets the variable, so CI can never pass by skipping.
- It copies the image and `eng.traineddata` into a temp directory named
  `scriptorium-ocr-tést`, and points `--tessdata-dir` there through the debug override. This runs
  both the relative image path and an absolute non-ASCII tessdata path, which covers decision 7
  on all three platforms.
- `ci.yml`: the `rust` job on Linux and the `rust-cross` matrix on macOS and Windows set
  `SCRIPTORIUM_REQUIRE_TESSERACT=1`.

### Chunk 5: sidecar config and bundle checks

- `tauri.conf.json` gains `bundle.externalBin: ["binaries/scriptorium-tesseract"]` and the
  resource `"tessdata/eng.traineddata": "tessdata/eng.traineddata"` in the base config. Tauri
  merges the per-OS configs as a JSON Merge Patch, so the `resources` objects merge key by key,
  and the notices, pdfium and tessdata all survive. The bundle checks below assert all three.
- The before-commands become exactly:
  - `beforeDevCommand`: `pnpm pdfium:fetch && pnpm tesseract:fetch && pnpm dev`
  - `beforeBuildCommand`: `pnpm pdfium:fetch && pnpm tesseract:fetch && pnpm licenses:check && pnpm build`

  On Windows, Tauri runs them through `cmd`, so `bash` must be on `PATH`. Git Bash provides it
  on the runners. `docs/development.md` says so for local Windows builds, as it already does
  for `pdfium:fetch`.
- `tauri-build` fails to compile when a listed `externalBin` file is missing. Every CI job that
  compiles `src-tauri` already runs the fetch first. The licences job does not compile.
- The sidecar smoke run is the same everywhere. Copy `tests/fixtures/ocr/smoke.png` into an
  empty temp directory. From there, run the bundled sidecar with `env -i` (or a cleared
  environment on Windows), `--tessdata-dir <the bundle's tessdata>`, `--psm 3 --oem 1`,
  `-c tessedit_create_tsv=1 -c tessedit_create_txt=0`. Assert that `out.tsv` contains `PRINCESS`,
  as `tesseract.yml` does.
- macOS, run by hand: `pnpm tauri build`. In the `.app`, assert that
  `Contents/MacOS/scriptorium-tesseract` exists and is executable, and that
  `Contents/Resources/tessdata/eng.traineddata` exists. `codesign -dv` must exit 0 on the
  sidecar and report the same signature kind as the main binary (ad hoc under
  `signingIdentity: "-"`). Record the flags it prints, but do not assert on the hardened
  runtime flag. On macOS 27 it is present even under the ad-hoc identity. `docs/ocr.md` records
  that, and notes that the full check waits for a real identity. Then do the smoke run. Record the output in the PR.
- New workflow, `.github/workflows/bundle.yml`, copying the `ci.yml` header pattern:
  `permissions: contents: read`, a `concurrency` group that cancels superseded runs, actions
  pinned by SHA, and `timeout-minutes: 45`. It runs on `workflow_dispatch`, weekly by
  `schedule`, and on PRs whose paths touch `scripts/fetch_tesseract.sh`,
  `scripts/fetch_pdfium*`, `src-tauri/tauri*.conf.json`, `src-tauri/build.rs`,
  `src-tauri/Cargo.toml`, `src-tauri/src/ocr/**`, `package.json`, or the workflow itself.
  It caches the pnpm store, the Rust build (`Swatinem/rust-cache`), and the fetched assets
  through `TESSERACT_ASSET_CACHE`, as `ci.yml` does.
  - Linux (`ubuntu-22.04`) installs the same packages as the `rust` job, plus `libfuse2` only if
    the AppImage build needs it. It runs `pnpm tauri build --bundles deb,appimage`. It unpacks the
    `.deb` with `dpkg-deb -x` and asserts `usr/bin/scriptorium-tesseract` and
    `usr/lib/Scriptorium/tessdata/eng.traineddata`. It unpacks the AppImage with
    `--appimage-extract` and asserts the same paths under `squashfs-root/`. It runs the smoke run
    from each tree.
  - Windows (`windows-latest`) runs `pnpm tauri build --bundles nsis`. NSIS is the authoritative
    Windows target for this check. It asserts `target/release/scriptorium-tesseract.exe` and
    `target/release/tessdata/eng.traineddata`, which is the tree NSIS packages. It then
    installs the NSIS installer silently (`/S /D=<temp dir>`), asserts the same two files in the
    install directory, and does the smoke run from there.
- Phase 4 must call `TesseractEngine::locate(app.path().resource_dir().ok().as_deref())`. This
  chunk leaves that unwired.

### Chunk 6: notices, policy, CVEs, docs

- `THIRD_PARTY_NOTICES.md`: Tesseract and the traineddata (Apache-2.0), Leptonica (BSD-2-Clause),
  zlib, libpng, libjpeg-turbo (IJG, BSD-3-Clause, zlib), libtiff, and libwebp (BSD-3-Clause), with
  versions.
- `docs/ci-and-hooks.md`: the Tesseract bump policy. A bump changes `build_tesseract.sh`, runs the
  build on the PR, publishes a new release tag after approval, updates the pins in
  `fetch_tesseract.sh` and the `id()` constants, and reruns `ocr_spike.py` if the engine version
  changed.
- `check_bundled_c_cves.py` and `ci/bundled-c.toml` from decision 11, with tests for the new
  source kind in the script's existing test file. `security.yml`'s `bundled-c-cves` job only
  runs on a PR when a file in its path regex changed. Add `scripts/build_tesseract.sh` to that
  regex, so a library bump triggers the check.
- `docs/development.md`, `docs/ocr.md`, both changelogs, and the parent plan's Phase 3 boxes.

## Verification

- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass locally with the fetched
  binary, and the integration test runs rather than skips.
- CI: the `rust`, `rust-cross`, `rust-coverage`, `tesseract`, and new `bundle` jobs pass.
- The macOS bundle check from Chunk 5 is recorded in the PR.
- Coverage stays at or above the 80% line floor.

## Risks

- **A broken asset blocks every build.** Chunk 5 makes the sidecar required at compile time. If a
  release asset is pulled or GitHub is down, Rust CI fails. The asset cache limits this, and the
  fetch retries three times.
- **Windows path handling.** If Tesseract or Leptonica cannot open a non-ASCII `--tessdata-dir`,
  Chunk 4 fails on Windows. The fallback in decision 7 is ready.
- **Bundle layout differs by format.** Chunk 5's job may need a different path per bundle format.
  The job records the path it checked, so a layout change shows up as a clear failure.
