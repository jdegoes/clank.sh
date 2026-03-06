# Development Workflow Reference

All development artifacts live in `dev-docs/`. Everything is a Markdown file with YAML frontmatter.

## Document Types

| Type | Location | Purpose |
|---|---|---|
| Research | `dev-docs/research/` | Raw investigation and prior art. Research informs but does not decide. |
| Design | `dev-docs/designs/proposed/` or `approved/` | Specifications for system areas. Approved designs are frozen permanent record. |
| Issue | `dev-docs/issues/open/` or `closed/` | What needs to be built or fixed, and why. No solution detail. |
| Plan | `dev-docs/plans/proposed/`, `approved/`, or `done/` | How an issue will be resolved. Full provenance: originating issue, research consulted, designs referenced, developer feedback on design decisions, acceptance tests. |

## Frontmatter Schema

Every document begins with YAML frontmatter:

| Field | Research | Design | Issue | Plan |
|---|---|---|---|---|
| `title` | required | required | required | required |
| `date` | required | required | required | required |
| `author` | required | required | required | required |
| `issue` | — | — | — | required — path to originating issue |
| `research` | — | — | — | required if applicable — list of paths |
| `designs` | — | — | — | required if applicable — list of paths |
| `closed` | — | — | optional — date closed | — |
| `plan` | — | — | optional — path to plan | — |
| `completed` | — | — | — | optional — date all tasks completed |
| `realized_design` | — | — | — | optional — path to realized design doc |

Lifecycle state is encoded in directory position, not frontmatter. No `status` field.
Use `author: agent` for agent-authored documents. ISO 8601 dates (`YYYY-MM-DD`).
Files are named in `kebab-case`, e.g. `lexer-unicode-support.md`.

## Lifecycle

1. **Issue** created in `dev-docs/issues/open/`. States the problem. Never modified to include solution detail.
2. **Research** conducted as needed in `dev-docs/research/`. If design docs for the area are missing, write a proposed design and note in the plan that it is pending human approval.
3. **Plan** written in `dev-docs/plans/proposed/`. Consult the developer on significant design decisions and record their feedback. Must include a `## Tasks` checkbox list.
4. **Human approves plan** by moving it to `dev-docs/plans/approved/`. No implementation begins before this. If a proposed design was written in step 2, the human approves or rejects it first; if rejected, revise and resubmit before the plan proceeds.
5. **Implementation proceeds.** Check off tasks as they complete. Record deviations inline. If the plan turns out to be wrong or incomplete, stop and ask the human — do not proceed unilaterally.
6. **Acceptance tests pass.** Once all pass, write a complete realized design doc to `dev-docs/designs/proposed/`. Do not summarize it; the human will review it in full.
7. **Human approves realized design** by moving it to `dev-docs/designs/approved/`. It supersedes the approved design for future reference. The original remains as permanent record.
8. **Closeout:** plan moved to `dev-docs/plans/done/`. Issue moved to `dev-docs/issues/closed/`. Both immutable from this point.

## Rules

- Agents write; humans gate moves into `approved/` and `done/`.
- Approved documents are never modified. They are permanent historical record.
- The code is the ground truth. Design docs record intent at a point in time, not a live mirror.
- Agents must never modify files in `dev-docs/plans/approved/`, `dev-docs/plans/done/`,
  `dev-docs/issues/closed/`, or `dev-docs/designs/approved/`.
- Agents must not create a plan without an originating issue in `dev-docs/issues/open/`.
