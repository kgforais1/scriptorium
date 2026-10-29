# Plans

Last reviewed: 2026-10-28

Implementation plans for plan-driven / multi-agent work. Policy:
[`policies/plans-and-todos.md`](../policies/plans-and-todos.md). Template:
[`templates/plan.md`](../templates/plan.md).

## Layout

| Path | Purpose |
|---|---|
| `*.md` (this directory) | **Active** plans and the living product roadmap |
| `orchestration-state.md` | Optional hub-and-spoke run state (orchestrator-owned) |
| `deferred/` | Plans intentionally postponed; still discoverable, not in progress |
| `archive/completed/` | Finished plans (`Status: complete`) — keep for history |
| `archive/superseded/` | Replaced or abandoned plans (`Status: abandoned` / superseded) |

Do **not** delete archived plans. Move them; do not rewrite history in place.

## Scriptorium active set

| Plan | Role |
|---|---|
| [`2026-08-23-product-roadmap.md`](2026-08-23-product-roadmap.md) | Product roadmap M0–M8 (stays active) |
| [`2026-08-23-m1-scan-viewer.md`](2026-08-23-m1-scan-viewer.md) | First product milestone (unblocked by ADR-0001) |

Completed: [`archive/completed/2026-08-23-m0-tauri-foundation.md`](archive/completed/2026-08-23-m0-tauri-foundation.md),
[`archive/completed/2026-08-23-pre-m1-flipbook-spike.md`](archive/completed/2026-08-23-pre-m1-flipbook-spike.md) ([ADR-0001](../docs/adr/0001-flipbook-engine.md)).
Superseded: [`archive/superseded/2026-08-23-mvp-scaffold.md`](archive/superseded/2026-08-23-mvp-scaffold.md).
Backlog index: [`to_do.md`](../to_do.md) — **Next Up** (3–5 pointers) → **Active plans** → **Icebox**.

## Backlog sync (agents)

When you create, complete, defer, or supersede a plan, update [`to_do.md`](../to_do.md) in the
**same change**:

| Event | `to_do.md` action |
|---|---|
| New active plan | Add **Active plans** row; add to **Next Up** when ready to execute |
| Plan complete / superseded | Remove from **Next Up** and **Active**; archive the plan and log durable impact appropriately |
| Plan deferred | Remove from **Next Up** / **Active**; link from **Icebox** or `deferred/` |
| Session start | Read **Next Up** first; open linked plan for checklist detail |

Do not duplicate plan checklists into `to_do.md`. Policy:
[`policies/plans-and-todos.md`](../policies/plans-and-todos.md).

## Lifecycle (agents)

1. New work → `plans/YYYY-MM-DD-slug.md` with `Status: draft`; add row to `to_do.md` **Active plans**.
2. On approval → `approved` / `in-progress`; add to **Next Up** when executing (keep 3–5 items).
3. On finish → `complete`, move to `archive/completed/`; remove from **Next Up** / **Active**.
4. On replace/stop → `abandoned` (note why), move to `archive/superseded/`; update `to_do.md`.
5. On postpone → `deferred`, move to `deferred/`; link from **Icebox** in `to_do.md`.

Mark checklist items `[x]` only when verified. See policy for TODO/changelog cleanup.
