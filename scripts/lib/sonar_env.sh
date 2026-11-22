#!/usr/bin/env bash
# Shared SonarQube environment helpers for scripts/sonar_scan.sh and scripts/sonar_up.sh.
#
# Sourced, never run. Resolves SONAR_HOST_URL from the exported environment first, then the
# repo-root .env, then the default. The .env file is parsed, never sourced, so no line in it
# can execute. Also sets PYTHON to the repo virtualenv interpreter, or python3 when that is
# missing.

SONAR_ENV_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

PYTHON="${PYTHON:-${SONAR_ENV_ROOT}/.venv/bin/python}"
if [[ ! -x "${PYTHON}" ]]; then
  PYTHON="$(command -v python3)"
fi

# Read one KEY from .env without sourcing it (no command execution).
dotenv_get() {
  local key="$1"
  [[ -f "${SONAR_ENV_ROOT}/.env" ]] || return 0
  "${PYTHON}" - "${SONAR_ENV_ROOT}/.env" "$key" <<'PYEOF'
import sys
from pathlib import Path

path, wanted = sys.argv[1], sys.argv[2]
for raw in Path(path).read_text(encoding="utf-8").splitlines():
    line = raw.strip()
    if line.startswith("export "):
        line = line[7:].strip()
    if not line or line.startswith("#") or "=" not in line:
        continue
    key, value = line.split("=", 1)
    if key.strip() == wanted:
        value = value.strip().split(" #", 1)[0].strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
            value = value[1:-1]
        print(value)
        break
PYEOF
}

if [[ -z "${SONAR_HOST_URL:-}" ]]; then
  SONAR_HOST_URL="$(dotenv_get SONAR_HOST_URL)"
fi
SONAR_HOST_URL="${SONAR_HOST_URL:-http://localhost:9000}"
