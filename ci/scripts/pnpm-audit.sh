#!/usr/bin/env bash
# pnpm-audit.sh — run `pnpm audit --prod`, retrying registry outages only.
#
# Why this exists: on 2026-10-22 four `Dependency audit` runs died on npm
# infrastructure, not on findings — the bulk-advisory endpoint returned 503,
# corepack's pnpm download returned 503, and one run ended in
# "TimeoutError: The operation was aborted due to timeout". A registry flake
# must not look like a vulnerability report, and it must never pass the gate.
#
# Contract:
#   exit 0        audit ran and found nothing
#   exit <status> pnpm reported findings (no retry, status passed through)
#   exit 1        registry/tooling stayed unreachable after every attempt
#
# Usage: bash ci/scripts/pnpm-audit.sh   (from the repo root)
# Env:   PNPM_AUDIT_ATTEMPTS (default 3), PNPM_AUDIT_DELAY (default 15s, scaled by attempt)
set -uo pipefail

ATTEMPTS="${PNPM_AUDIT_ATTEMPTS:-3}"
DELAY="${PNPM_AUDIT_DELAY:-15}"
PNPM_VERSION="11.17.0"

# Registry-shaped failures. Kept narrow on purpose: a match triggers a retry, so
# these patterns must never appear in a report that lists found vulnerabilities.
TRANSIENT_RE='ERR_PNPM|ECONNRESET|ETIMEDOUT|EAI_AGAIN|ENOTFOUND|ENETUNREACH|socket hang up|TimeoutError|aborted due to timeout|error \(5[0-9][0-9]\)|error \(23\)'

fail() {
  printf '::error title=%s::%s\n' "$1" "$2" >&2
}

with_retries() {
  local what="$1"
  shift
  local attempt
  for ((attempt = 1; attempt <= ATTEMPTS; attempt++)); do
    if "$@"; then
      return 0
    fi
    if ((attempt < ATTEMPTS)); then
      printf '::warning title=Transient registry failure::%s failed (attempt %d/%d), retrying\n' \
        "$what" "$attempt" "$ATTEMPTS" >&2
      sleep $((attempt * DELAY))
    fi
  done
  fail "Audit tooling unavailable" "$what failed $ATTEMPTS times"
  return 1
}

# corepack fetches pnpm from the same registry that flaked.
with_retries "corepack enable" corepack enable || exit 1
with_retries "corepack prepare pnpm@$PNPM_VERSION" \
  corepack prepare "pnpm@$PNPM_VERSION" --activate || exit 1

# pnpm exits non-zero both for "vulnerabilities found" and for a registry error,
# so classify by output instead of by exit code.
for ((attempt = 1; attempt <= ATTEMPTS; attempt++)); do
  output=""
  status=0
  output=$(pnpm audit --prod 2>&1) || status=$?

  if ((status == 0)); then
    printf '%s\n' "$output"
    exit 0
  fi

  printf '%s\n' "$output" >&2

  if ! grep -qiE "$TRANSIENT_RE" <<<"$output"; then
    fail "Vulnerabilities found" "pnpm audit reported findings; see log above"
    exit "$status"
  fi

  if ((attempt < ATTEMPTS)); then
    printf '::warning title=Transient registry failure::pnpm audit hit the registry (attempt %d/%d), retrying\n' \
      "$attempt" "$ATTEMPTS" >&2
    sleep $((attempt * DELAY))
  fi
done

fail "Audit did not run" \
  "pnpm audit could not reach the registry after $ATTEMPTS attempts; treat as infrastructure, not a clean audit"
exit 1
