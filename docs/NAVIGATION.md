# Documentation Navigation

Last reviewed: 2026-10-28

Quick navigation guide for this template's documentation. Use this to find what you need without loading everything.

## Quick Start by Role

### I'm starting a new project
1. Read [`README.md`](../README.md) (2 min)
2. Run [`scripts/setup.sh`](../scripts/setup.sh) (1 min)
3. Feed [`prompts/bootstrap-project.md`](../prompts/bootstrap-project.md) to your AI agent
4. Remove irrelevant content (see setup script cleanup suggestions)

### I'm returning to an existing project
1. Run [`scripts/health-check.sh`](../scripts/health-check.sh) (30 sec)
2. Feed [`prompts/new-agent-session.md`](../prompts/new-agent-session.md) to your AI agent
3. Check `.context/project-profile.md` if it exists

### I want to understand this template
1. Read [`AGENTS.md`](../AGENTS.md) (3 min)
2. Browse [`inventory/README.md`](../inventory/README.md) for available tools
3. Skim [`README.md`](../README.md) sections

### I'm setting up CI/CD
1. Read [`ci/README.md`](../ci/README.md)
2. Copy relevant workflows from [`ci/examples/`](../ci/examples/)
3. Configure per [`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md)

### I'm handling credentials or confidential data
1. Read [`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md) for data classification
2. Enable gitleaks per [`hooks/README.md`](../hooks/README.md)
3. Wire required secret-scan CI checks before merging to the default branch

## Documentation Map

### Core Navigation (Read These First)
- [`AGENTS.md`](../AGENTS.md) - Single source of truth for AI agents
- [`README.md`](../README.md) - Project overview and directory map
- [`inventory/README.md`](../inventory/README.md) - Tool/skill menu (not a checklist)

### Domain
- [`docs/book-format.md`](../docs/book-format.md) — book package format spec (formatVersion 1); schema + ingest rules
- [`docs/book-format.schema.json`](../docs/book-format.schema.json) — JSON Schema for `meta.json`
- [`tests/fixtures/README.md`](../tests/fixtures/README.md) — regenerate fixtures via `scripts/generate-fixture-book.py`
- [`src-tauri/`](../src-tauri/) — Tauri 2 Rust host (`book_format` types)

### Getting Started
- [`prompts/bootstrap-project.md`](../prompts/bootstrap-project.md) - New project initialization
- [`prompts/bootstrap-checklist.md`](../prompts/bootstrap-checklist.md) - Tick-list version (phases P0–P8)
- [`prompts/bootstrap/`](../prompts/bootstrap/) - Decision cards: ask → branch → produce → done when
- [`templates/bootstrap-state.md`](../templates/bootstrap-state.md) - Phase status that survives a context reset
- [`scripts/check-bootstrap.sh`](../scripts/check-bootstrap.sh) - Verify phases by repo evidence, not by claims
- [`prompts/new-agent-session.md`](../prompts/new-agent-session.md) - Returning to existing projects
- [`scripts/setup.sh`](../scripts/setup.sh) - Automated setup
- [`scripts/health-check.sh`](../scripts/health-check.sh) - Project health check

### Project Configuration
- [`prompts/project-init-profile.md`](../prompts/project-init-profile.md) - Capture project type and stack
- `.context/project-profile.md` - Generated project profile (fast-load summary; gitignored, so it does not exist in the template itself)

### Policies & Rules
- [`policies/README.md`](../policies/README.md) - Durable repo rules index
- [`policies/file-size-and-counts.md`](../policies/file-size-and-counts.md) - File size limits
- [`policies/plans-and-todos.md`](../policies/plans-and-todos.md) - Plan lifecycle and TODO limits
- [`policies/changelog-conventions.md`](../policies/changelog-conventions.md) - Changelog format
- [`policies/security-baseline.md`](../policies/security-baseline.md) - Security requirements
- [`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md) - GitHub setup and secret gates

### Hooks & Automation
- [`hooks/README.md`](../hooks/README.md) - Pre-commit hooks and policy scripts
- [`hooks/.pre-commit-config.yaml`](../hooks/.pre-commit-config.yaml) - Example configuration
- [`scripts/validate-env.sh`](../scripts/validate-env.sh) - Environment validation
- [`docs/sonarqube.md`](../docs/sonarqube.md) - Opt-in local SonarQube scan and freshness gate

### CI/CD
- [`ci/README.md`](../ci/README.md) - CI selection guidance
- [`ci/examples/`](../ci/examples/) - Example GitHub Actions workflows
- [`policies/github-actions-usage.md`](../policies/github-actions-usage.md) - Actions minutes/storage

### Templates & Artifacts
- [`templates/README.md`](../templates/README.md) - Fill-in artifacts index
- [`templates/project-brief.md`](../templates/project-brief.md) - Project briefs
- [`templates/plan.md`](../templates/plan.md) - Implementation plans
- [`templates/design.md`](../templates/design.md) - Design documents
- [`templates/adr.md`](../templates/adr.md) - Architecture Decision Records
- [`adr/0001-flipbook-engine.md`](adr/0001-flipbook-engine.md) - ADR-0001: page-turn engine (WebGL2 port of mockup E)
- [`templates/bootstrap-state.md`](../templates/bootstrap-state.md) - Bootstrap phase status
- [`templates/handoff.md`](../templates/handoff.md) - Cross-agent / cross-IDE handoff packet

### Maintenance & Cleanup
- [`prompts/maintenance-loop.md`](../prompts/maintenance-loop.md) - Periodic repo health checks
- [`prompts/cleanup-completed-work.md`](../prompts/cleanup-completed-work.md) - Cleanup completed items
- [`policies/plans-and-todos.md`](../policies/plans-and-todos.md) - Archiving completed plans

### Inventory Menus (Load Only What You Need)
- [`inventory/tools-index.md`](../inventory/tools-index.md) - General developer tools
- [`inventory/security-quality.md`](../inventory/security-quality.md) - Security and quality tools
- [`inventory/python.md`](../inventory/python.md) - Python project defaults
- [`inventory/frontend-design-ux.md`](../inventory/frontend-design-ux.md) - Frontend and design
- [`inventory/rag.md`](../inventory/rag.md) - RAG and vector databases
- [`inventory/ai-agent-platforms.md`](../inventory/ai-agent-platforms.md) - Agent orchestration
- [`inventory/cloud-and-infra.md`](../inventory/cloud-and-infra.md) - Cloud platforms
- [`catalog-skills-agents.md`](../inventory/catalog-skills-agents.md) - Skills and agents catalog

### Harness Engineering
- [`inventory/harness-engineering.md`](../inventory/harness-engineering.md) - Agent platform references
- [`inventory/catalog-skills-agents.md`](../inventory/catalog-skills-agents.md) - Skills/agents installation
- [`inventory/agent-tooling-efficiency.md`](../inventory/agent-tooling-efficiency.md) - Token-efficient tooling; choosing between overlapping code-intelligence tools
- [`policies/agent-tooling-contract.md`](../policies/agent-tooling-contract.md) - The rule to paste into a project's `AGENTS.md`

## Decision Trees

### Choosing Orchestration Tier
→ See [`inventory/harness-engineering.md`](../inventory/harness-engineering.md) decision table
- Simple task → No orchestration
- 3-10 agents, IDE workflow → Hub-and-spoke
- Complex loops → LangGraph
- Production API → Symphony

### Choosing CI Checks
→ See [`ci/README.md`](../ci/README.md) tier guidance
- Fast checks (<5 min) → Pre-commit + fast CI lane
- Slow checks → Scheduled CI or separate workflow
- Agent reviews → Never gates, always advisory

### Data Classification
→ See [`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md)
- Public → Basic secret scanning
- Internal → Standard branch protection + secret scan
- Confidential → Full hygiene tier + required secret-scan CI

## Search by Task

| Task | Documentation |
|---|---|
| **Start new project** | `prompts/bootstrap-project.md` + `scripts/setup.sh` |
| **Add pre-commit hooks** | `hooks/README.md` |
| **Set up CI** | `ci/README.md` + `ci/examples/` |
| **Write ADR** | `templates/adr.md` |
| **Protect secrets in git** | `hooks/README.md` + `policies/github-repository-hygiene.md` |
| **Choose tools** | `inventory/README.md` (then specific inventory files) |
| **Clean up completed work** | `prompts/cleanup-completed-work.md` |
| **Add agent skills** | `inventory/catalog-skills-agents.md` |
| **Configure GitHub** | `policies/github-repository-hygiene.md` |
| **Check project health** | `scripts/health-check.sh` |
| **Validate environment** | `scripts/validate-env.sh` |
| **Resume an unfinished bootstrap** | `.context/bootstrap-state.md` + `scripts/check-bootstrap.sh` |
| **Hand work to another agent/IDE** | `templates/handoff.md` |
| **Add code-intelligence / MCP tooling** | `inventory/agent-tooling-efficiency.md` |
| **Find stale tool entries / dead links** | `ci/scripts/check_doc_links.py` |

## File Size Quick Reference

Per [`policies/file-size-and-counts.md`](../policies/file-size-and-counts.md):
- **Source files**: Soft 600 lines / Hard 1000 lines
- **Markdown docs**: Soft 1000 lines / Hard 1000 lines  
- **Binary files**: 5 MB hard limit
- **Non-source files**: 1 MB hard limit

## Common Commands

```bash
# Setup
bash scripts/setup.sh

# Health check
bash scripts/health-check.sh

# Environment validation
bash scripts/validate-env.sh

# Pre-commit
pre-commit install
pre-commit run --all-files

# Check GitHub Actions usage
python3 ci/scripts/check_gha_usage.py --repo
```

## Tips

1. **Don't load everything** - Start with AGENTS.md, then open only what you need
2. **Menu, not checklist** - Inventory files are options, not requirements
3. **Thin entry, deep docs** - AGENTS.md is short; detail lives in linked files
4. **Temporary stays temporary** - Use `.context/` for scratch work
5. **Clean up after completion** - Archive completed plans, remove completed TODOs
