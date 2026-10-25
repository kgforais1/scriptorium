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
#   exit <status> pnpm reported findings — classified first, never retried
#   exit 127      pnpm missing from PATH after corepack
#   exit 1        registry/tooling stayed unreachable, or the retry env is invalid
#
# Usage: bash ci/scripts/pnpm-audit.sh   (from the repo root)
# Env:   PNPM_AUDIT_ATTEMPTS (default 3, integer 1–5, at most 3 digits)
#        PNPM_AUDIT_DELAY    (default 15, integer seconds 0–120, scaled by attempt)
#
# corepack enable is local (shim setup) and is not retried. corepack prepare
# downloads pnpm from the registry: only registry-shaped output is retried.
# The pnpm version is package.json's packageManager field, not a second pin.
set -uo pipefail

ATTEMPTS="${PNPM_AUDIT_ATTEMPTS:-3}"
DELAY="${PNPM_AUDIT_DELAY:-15}"

# Registry-shaped failures only. Do not use a bare ERR_PNPM prefix: pnpm uses
# that prefix for non-transient errors too, including some audit failures.
# pnpm 11.17.0 reports a non-200 audit response as ERR_PNPM_AUDIT_BAD_RESPONSE
# with "responded with <status>". Match that 5xx phrase only — the same code
# is also used for invalid JSON, which is not a registry retry.
# Findings are detected separately and always win over this pattern.
TRANSIENT_RE='ECONNRESET|ETIMEDOUT|EAI_AGAIN|ENOTFOUND|ENETUNREACH|socket hang up|TimeoutError|aborted due to timeout|error \(5[0-9][0-9]\)|error \(23\)|HTTP 503|responded with 5[0-9][0-9]'

fail() {
  printf '::error title=%s::%s\n' "$1" "$2" >&2
}

# At most 3 digits, then force base-10. A longer digit string wraps in 64-bit
# arithmetic (2^64+k can look like a small k). Bash also evaluates expressions,
# so a value like `1+$(cmd)` would otherwise run inside $((...)).
if ! [[ "$ATTEMPTS" =~ ^[0-9]{1,3}$ && "$DELAY" =~ ^[0-9]{1,3}$ ]]; then
  fail "Audit tooling unavailable" \
    "PNPM_AUDIT_ATTEMPTS and PNPM_AUDIT_DELAY must be non-negative integers (at most 3 digits)"
  exit 1
fi
ATTEMPTS=$((10#$ATTEMPTS))
DELAY=$((10#$DELAY))
if ((ATTEMPTS < 1 || ATTEMPTS > 5 || DELAY > 120)); then
  fail "Audit tooling unavailable" \
    "PNPM_AUDIT_ATTEMPTS must be 1-5 and PNPM_AUDIT_DELAY must be 0-120"
  exit 1
fi

# One version source: the top-level packageManager field, as JSON, so a
# compact or pretty package.json both work. An optional +sha512 suffix is
# stripped before corepack prepare.
if [[ ! -f package.json ]]; then
  fail "pnpm setup failed" "package.json is missing; cannot read packageManager"
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  fail "pnpm setup failed" "python3 is required to read package.json packageManager"
  exit 1
fi
status=0
PNPM_VERSION=$(
  python3 - <<'PY'
import json
import sys

try:
    with open("package.json", encoding="utf-8") as handle:
        data = json.load(handle)
except (OSError, json.JSONDecodeError) as exc:
    print(f"package.json: {exc}", file=sys.stderr)
    sys.exit(2)
if not isinstance(data, dict):
    print("package.json must be a JSON object", file=sys.stderr)
    sys.exit(2)
raw = data.get("packageManager")
if not isinstance(raw, str):
    print('package.json packageManager must be "pnpm@<version>"', file=sys.stderr)
    sys.exit(2)
name, sep, rest = raw.partition("@")
version = rest.split("+", 1)[0]
if sep != "@" or name != "pnpm" or not version:
    print('package.json packageManager must be "pnpm@<version>"', file=sys.stderr)
    sys.exit(2)
print(version)
PY
) || status=$?
if ((status != 0)); then
  fail "pnpm setup failed" \
    'package.json packageManager must be "pnpm@<version>"; see log above'
  exit 1
fi
if ! [[ "$PNPM_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z]+)*$ ]]; then
  fail "pnpm setup failed" "package.json packageManager version is not a pnpm version"
  exit 1
fi

# enable only links the local shim. It is not a registry call.
if ! corepack enable; then
  fail "Audit tooling unavailable" "corepack enable failed"
  exit 1
fi

# prepare is the registry download. A permanent failure (bad flag, unknown
# version, local corepack error) is not a flake and must not be retried.
prepared=0
for ((attempt = 1; attempt <= ATTEMPTS; attempt++)); do
  prepare_output=""
  status=0
  prepare_output=$(corepack prepare "pnpm@$PNPM_VERSION" --activate 2>&1) || status=$?
  if ((status == 0)); then
    printf '%s\n' "$prepare_output"
    prepared=1
    break
  fi
  printf '%s\n' "$prepare_output" >&2
  if ! grep -qiE "$TRANSIENT_RE" <<<"$prepare_output"; then
    fail "pnpm setup failed" \
      "corepack prepare failed and the output is not a registry error; see log above"
    exit "$status"
  fi
  if ((attempt < ATTEMPTS)); then
    printf '::warning title=Transient registry failure::corepack prepare hit the registry (attempt %d/%d), retrying\n' \
      "$attempt" "$ATTEMPTS" >&2
    sleep $((attempt * DELAY))
  fi
done
if ((prepared == 0)); then
  fail "Audit tooling unavailable" \
    "corepack prepare could not reach the registry after $ATTEMPTS attempts"
  exit 1
fi

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

  # Findings win over every other classification: a report that also mentions a
  # 503 or a timeout is still a findings report, and must never be retried.
  if grep -qiE 'vulnerabilit(y|ies) found' <<<"$output"; then
    fail "Vulnerabilities found" "pnpm audit reported findings; see log above"
    exit "$status"
  fi

  # A missing binary is a broken toolchain, not a finding.
  if ((status == 127)); then
    fail "Audit tooling unavailable" "pnpm is not on PATH after corepack prepare"
    exit 127
  fi

  if ! grep -qiE "$TRANSIENT_RE" <<<"$output"; then
    fail "pnpm audit failed" "pnpm audit exited $status with no registry error; see log above"
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
