#!/usr/bin/env bash
# Bring the local SonarQube server up after a reboot. Never run by CI or hooks.
#
# If the server already reports UP, this exits at once. Otherwise it starts Colima when Docker
# is unreachable, then starts the containers that already exist. Creating them for the first
# time needs SONAR_DB_PASSWORD in the repo-root .env, since compose.sonarqube.yaml requires it.
# Polls /api/system/status every 5 s, for at most 5 minutes.
#
# Prerequisites and troubleshooting: docs/sonarqube.md.
#
# Usage: bash scripts/sonar_up.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SONAR_HOST_URL="${SONAR_HOST_URL:-http://localhost:9000}"
COMPOSE_FILE="compose.sonarqube.yaml"
TIMEOUT_SECONDS=300
POLL_SECONDS=5

# True when the server answers and reports UP, within max_time seconds (default 5).
server_is_up() {
  local max_time="${1:-5}" status
  status="$(curl -fsS --max-time "$max_time" "$SONAR_HOST_URL/api/system/status" 2> /dev/null)" || return 1
  [[ "$status" == *'"status":"UP"'* ]]
}

if server_is_up; then
  echo "sonar_up: SonarQube is already UP at $SONAR_HOST_URL"
  exit 0
fi

if ! docker info > /dev/null 2>&1; then
  if command -v colima > /dev/null 2>&1; then
    echo "sonar_up: Docker is not reachable; starting Colima"
    colima start
  else
    echo "sonar_up: Docker is not reachable and colima is not on PATH." >&2
    exit 1
  fi
  if ! docker info > /dev/null 2>&1; then
    echo "sonar_up: Docker is still unreachable after colima start." >&2
    exit 1
  fi
fi

# The compose file requires ${SONAR_DB_PASSWORD:?...}, and Compose interpolates it for every
# subcommand, so both `ps` and `start` fail without the password. Find the project through
# Compose when that works, otherwise through Docker's own compose labels.
project=""
if compose_ids="$(docker compose -f "$COMPOSE_FILE" ps -a -q 2> /dev/null)"; then
  first_id="$(printf '%s\n' "$compose_ids" | sed -n '1p')"
  if [[ -n "$first_id" ]]; then
    project="$(docker inspect --format '{{index .Config.Labels "com.docker.compose.project"}}' \
      "$first_id" 2> /dev/null || true)"
  fi
fi

if [[ -z "$project" ]]; then
  # The project name is static in the file, so read it without asking Compose to interpolate.
  project="$(sed -n 's/^name:[[:space:]]*//p' "$COMPOSE_FILE" | sed -n '1p')"
  if [[ -n "$project" ]]; then
    project_ids="$(docker ps -a -q --filter "label=com.docker.compose.project=$project")"
    [[ -n "$project_ids" ]] || project=""
  fi
fi

if [[ -z "$project" ]]; then
  # docs/sonarqube.md allows one SonarQube server per machine. Reuse the single existing stack
  # rather than creating a second one that needs the database password.
  projects="$(docker ps -a --format '{{.Label "com.docker.compose.project"}}' \
    --filter 'label=com.docker.compose.service=sonarqube' | sort -u | sed '/^$/d')"
  first="$(printf '%s\n' "$projects" | sed -n '1p')"
  extra="$(printf '%s\n' "$projects" | sed -n '2,$p')"
  if [[ -n "$first" && -z "$extra" ]]; then
    project="$first"
    echo "sonar_up: reusing the existing stack in compose project $project"
  fi
fi

# `docker start` fails on a running container, so start only the stopped ones, and only for the
# stack's own services. A shared project may hold unrelated containers the user stopped.
server_hint="docker ps -a --filter label=com.docker.compose.service=sonarqube"
if [[ -n "$project" ]]; then
  started=0

  # start_service SERVICE : starts that service's exited or created containers, if any.
  start_service() {
    local service="$1" ids
    ids="$(
      {
        docker ps -a -q --filter "label=com.docker.compose.project=$project" \
          --filter "label=com.docker.compose.service=$service" --filter status=exited
        docker ps -a -q --filter "label=com.docker.compose.project=$project" \
          --filter "label=com.docker.compose.service=$service" --filter status=created
      } | sed '/^$/d' | sort -u
    )"
    if [[ -n "$ids" ]]; then
      echo "sonar_up: starting $service"
      # Word splitting is wanted: docker start takes one argument per container ID.
      # shellcheck disable=SC2086
      docker start $ids > /dev/null
      started=1
    fi
  }

  # The database first, so the server finds it up when it starts.
  start_service db
  start_service sonarqube
  if [[ "$started" == 0 ]]; then
    echo "sonar_up: db and sonarqube are already running"
  fi

  server_id="$(docker ps -a -q --filter "label=com.docker.compose.project=$project" \
    --filter 'label=com.docker.compose.service=sonarqube' | sed -n '1p')"
  if [[ -n "$server_id" ]]; then
    server_hint="docker logs $server_id"
  fi
else
  # First run on this machine. Compose's own error explains the missing password.
  echo "sonar_up: creating the containers from $COMPOSE_FILE"
  docker compose -f "$COMPOSE_FILE" up -d
fi

echo "sonar_up: waiting for $SONAR_HOST_URL to report UP (up to 5 minutes)"
deadline=$((SECONDS + TIMEOUT_SECONDS))
attempt=0
while true; do
  remaining=$((deadline - SECONDS))
  if [[ "$remaining" -le 0 ]]; then
    break
  fi
  poll="$remaining"
  if [[ "$poll" -gt "$POLL_SECONDS" ]]; then
    poll="$POLL_SECONDS"
  fi
  if server_is_up "$poll"; then
    echo "sonar_up: SonarQube is UP at $SONAR_HOST_URL"
    exit 0
  fi
  attempt=$((attempt + 1))
  remaining=$((deadline - SECONDS))
  if [[ "$remaining" -le 0 ]]; then
    break
  fi
  poll="$remaining"
  if [[ "$poll" -gt "$POLL_SECONDS" ]]; then
    poll="$POLL_SECONDS"
  fi
  if [[ "$poll" -lt 1 ]]; then
    poll=1
  fi
  echo "sonar_up: not up yet (attempt $attempt); retrying in ${poll} s"
  sleep "$poll"
done

echo "sonar_up: timed out after 5 minutes." >&2
echo "  Check the server log: $server_hint" >&2
echo "  If it keeps exiting at startup, check vm.max_map_count in docs/sonarqube.md." >&2
exit 1
