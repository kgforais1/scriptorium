#!/usr/bin/env bash
# Manual, opt-in local SonarQube scan. Never run by CI or hooks.
#
# Prerequisites (see docs/sonarqube.md):
#   - a SonarQube server at SONAR_HOST_URL (default http://localhost:9000),
#     e.g. `docker compose -f compose.sonarqube.yaml up -d`
#   - SONAR_TOKEN exported, or SONAR_TOKEN=... in repo-root .env
#     (an exported value wins; .env is parsed, never sourced)
#   - `sonar-scanner` on PATH (brew install sonar-scanner)
#
# Usage: bash scripts/sonar_scan.sh [--coverage]
#   --coverage  run `pytest --cov --cov-report=xml` first so coverage.xml is fresh.
#               Without it, an existing coverage.xml is used if present.
#
# Scope lives in sonar-project.properties. After the upload, scripts/sonar_issues.py
# waits for the analysis, dumps issues under tmp/, and writes tmp/sonar-state.json
# for the opt-in freshness gate. The token goes to the scanner via the
# environment, never argv.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_COVERAGE=0
for arg in "$@"; do
  case "$arg" in
    --coverage) RUN_COVERAGE=1 ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) echo "sonar_scan: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

# shellcheck source=scripts/lib/sonar_env.sh
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/sonar_env.sh"

if [[ -z "${SONAR_TOKEN:-}" ]]; then
  SONAR_TOKEN="$(dotenv_get SONAR_TOKEN)"
fi

if [[ -z "${SONAR_TOKEN:-}" ]]; then
  echo "sonar_scan: SONAR_TOKEN is not set; export it or add it to repo-root .env." >&2
  exit 1
fi

# Local-only by design: refuse to ship the tree to a remote server by accident.
# The parsed-URL check lives in sonar_issues.py so both scripts agree. SONAR_ALLOW_REMOTE is
# read only from the exported environment, never from .env.
if ! SONAR_HOST_URL="$SONAR_HOST_URL" "$PYTHON" "$ROOT_DIR/scripts/sonar_issues.py" --check-host; then
  echo "sonar_scan: SONAR_HOST_URL must have no query or fragment, and must be loopback (http or https) or https with SONAR_ALLOW_REMOTE=1 exported." >&2
  exit 1
fi

if ! command -v sonar-scanner >/dev/null 2>&1; then
  echo "sonar_scan: sonar-scanner is not installed (brew install sonar-scanner)." >&2
  exit 1
fi

if ! curl -fsS --max-time 5 "$SONAR_HOST_URL/api/system/status" >/dev/null 2>&1; then
  echo "sonar_scan: no SonarQube server answering at $SONAR_HOST_URL." >&2
  echo "Start one: bash scripts/sonar_up.sh" >&2
  exit 1
fi

if [[ "$RUN_COVERAGE" == "1" ]]; then
  echo "== Generating coverage.xml (pytest) =="
  "$PYTHON" -m pytest --cov --cov-report=xml -q
fi

# Fingerprint HEAD and the working tree; sonar_issues.py records the scan only if it is unchanged.
SONAR_SCAN_FINGERPRINT="$("$PYTHON" "$ROOT_DIR/scripts/sonar_issues.py" --fingerprint)"
export SONAR_TOKEN SONAR_HOST_URL SONAR_SCAN_FINGERPRINT
echo "== sonar-scanner -> $SONAR_HOST_URL =="
sonar-scanner -Dsonar.projectVersion="$(tr -d '[:space:]' < "$ROOT_DIR/VERSION")"

echo "== Waiting for analysis and dumping issues =="
"$PYTHON" "$ROOT_DIR/scripts/sonar_issues.py"
