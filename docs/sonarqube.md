# Local SonarQube (opt-in)

Last reviewed: 2026-10-28

SonarQube is an optional local second opinion. Nothing in CI, GitHub Actions, or a
cloud agent needs a Sonar server or `SONAR_TOKEN`. The scan runs only when you run it.
The freshness hooks do nothing unless you turn them on.

SonarQube Cloud automatic analysis is separate. It reads only
[`.sonarcloud.properties`](../.sonarcloud.properties). The local scan reads
[`sonar-project.properties`](../sonar-project.properties), which has a wider scope.
It covers `src/`, `src-tauri/src/`, the harness scripts, and `tests/`.

## Start the server

One server can serve every repo on the machine. If another repo already runs one on
port 9000, reuse it and skip this step.

```bash
# .env: SONAR_DB_PASSWORD=<long random string>
docker compose -f compose.sonarqube.yaml up -d
```

PostgreSQL reads `SONAR_DB_PASSWORD` only when it first creates its volume. To change the
password later, delete the volume and start again. This also deletes all analysis history:

```bash
docker compose -f compose.sonarqube.yaml down -v
docker compose -f compose.sonarqube.yaml up -d
```

[`compose.sonarqube.yaml`](../compose.sonarqube.yaml) runs SonarQube Community Build
with PostgreSQL. It binds to `127.0.0.1:9000` only. Open `http://localhost:9000`, log in
as `admin`/`admin`, and change the password.

The server's Elasticsearch needs `vm.max_map_count` of at least 524288 on the Docker
host VM. Check that first if the container exits during startup.

## Set the token

Create a project with key `scriptorium`. Generate a token under
**My Account → Security**. Put it in the repo-root `.env` (gitignored):

```bash
SONAR_TOKEN=squ_...
# SONAR_HOST_URL=http://localhost:9000   # the default
```

An exported variable wins over `.env`. The scan script parses `.env` and never sources it.

## Run the scan

```bash
brew install sonar-scanner          # once; the gate also needs git 2.24 or later
bash scripts/sonar_scan.sh          # uses coverage.xml if present
bash scripts/sonar_scan.sh --coverage   # runs pytest --cov first
```

[`scripts/sonar_scan.sh`](../scripts/sonar_scan.sh) refuses a non-loopback host unless
`SONAR_ALLOW_REMOTE=1` is set. It passes the token through the environment, not argv.
Then [`scripts/sonar_issues.py`](../scripts/sonar_issues.py) waits for the server to
finish the analysis and writes these gitignored files:

- `tmp/sonar-latest-issues.md` is a one-line-per-issue summary.
- `tmp/sonar-latest-issues.json` holds the raw issues.
- `tmp/sonar-issues/<UTC>.{json,md}` keeps the last 20 dumps for comparison.
- `tmp/sonar-state.json` records the scanned commit for the freshness gate.

A failed analysis or API error leaves the previous state untouched. So does a tree that
changed during the scan: the scan fingerprints HEAD, uncommitted changes, and untracked
files before it starts, and records the state only if that fingerprint still matches.

## Read the issues

Start with `tmp/sonar-latest-issues.md`, or open the project in the web UI. Triage
bugs and vulnerabilities first. Compare issue identities between dumps, not only
totals, because one fix can hide one new bug. Do not paste raw dumps into a PR.

## Enable the freshness gate

Two pre-commit hooks call
[`scripts/check_sonar_freshness.py`](../scripts/check_sonar_freshness.py).
Both exit 0 at once, with no git or network calls, unless the gate is on:

```bash
# .env
SONAR_FRESHNESS_GATE=1
# SONAR_GATE_MAX_COMMITS=10
```

- `sonar-freshness` (commit) warns at 80% of the budget. It blocks once the commits
  since the last scan, plus this one, reach `SONAR_GATE_MAX_COMMITS`.
- `sonar-freshness-push` (pre-push) blocks if any commit landed since the last scan.
  It judges the pushed commit (`PRE_COMMIT_TO_REF`), not only `HEAD`.

The pre-push hook is active only after
`pre-commit install --hook-type pre-commit --hook-type pre-push`.
A rebase or amend moves the scanned commit out of history, so the gate blocks until
the next scan. To turn the gate off, remove the line or set it to `0`.
