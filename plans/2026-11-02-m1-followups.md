# Plan: M1 follow-ups after 0.2.0

Last reviewed: 2026-11-02
Date: 2026-11-02
Author: Claude (release 0.2.0)
Status: in-progress
Linked issue/PR: [#74](https://github.com/kgforais1/scriptorium/pull/74)
Follows: [M1 scan viewer](archive/completed/2026-08-23-m1-scan-viewer.md) (archived; history only)

## Goal

Close the checks that the M1 plan left open when 0.2.0 shipped. The main flows passed a manual
run on 2026-11-02. These items are the rest. Section links point at the archived plan for detail.

## Out of scope

- New features. Those belong to M1.5 and later in the [roadmap](2026-08-23-product-roadmap.md).

## Checklist

### Manual verification

- [ ] Run [`docs/manual-test-m1.md`](../docs/manual-test-m1.md) exhaustively on macOS, and record
      the results.
- [ ] Check acceptance A1–A9, plus the keyboard, accessible-name and reduced-motion checks.
- [ ] Take the A9 measurements on the Dulac corpus: open to first turn, RSS with the texture
      height, and mean fps over ten turns.
- [ ] Run a Windows release build under WebView2, including page paths and the
      `https://asset.localhost` form.

### Engine gate ([Phase 0](archive/completed/2026-08-23-m1-scan-viewer.md#phase-0--integration-gate-adr-already-done))

- [ ] Verify the [asset-protocol checklist](archive/completed/2026-08-23-m1-scan-viewer.md#tauri-asset-protocol-checklist-required-before-reader-phase)
      against the shipped code, and tick each item that holds.
- [ ] Close ADR-0001's open items: WebGL2 on WebView2 and WebKitGTK, touch on real hardware,
      WKWebView frame rate, and an RSS sample.
- [ ] Mark the `rust-cross` CI job required on Windows and macOS in the branch ruleset.
- [ ] Decide single versus spread layout defaults.

### Hardening ([M0 review follow-ups](archive/completed/2026-08-23-m1-scan-viewer.md#follow-ups-from-m0-senior-reviews-2026-08-25))

- [x] Confirm A10–A12 have tests (named in [M1 hardening](2026-11-05-m1-hardening.md) findings; no new Rust tests).
- [x] Static pytest locks production CSP, empty asset scope, and default capabilities (`tests/test_tauri_csp.py`).
- [ ] Re-check A13 CSP in a release build (manual script section 8; Windows WebView2 bullet above).
- [x] Add a debug `cargo build` to CI (LC-M2).
- [ ] Consider a `pageLabel` `maxLength` (LC-T11).

## Verification

- [ ] Every item above is ticked or moved to a later plan with a reason.
