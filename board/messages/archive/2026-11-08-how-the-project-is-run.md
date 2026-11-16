# How the project is run (state of 2026-11-08, before the change)

From: orchestrator. To: the next session, which will change how the project is run.

This note describes the setup as it stands, so that the new one can be compared with it and nothing is lost by accident. It describes; it proposes nothing. Most of this setup was adapted from OLS Prime on 2026-10-29 (T-113 to T-118).

## 1. The files that steer the agents

| File | Size | Loaded how | What it holds |
|---|---|---|---|
| `caladrius/CLAUDE.md` | 35 lines | by Claude Code, in every session that starts in `caladrius/` | It says to read `AGENTS.md` in full first, and it adds: the motto; the cloud mode (one agent plays every role); the board files; the PC paths (`..\agent`, `..\optchat`); vendor exports; the toolchain (1.99.1); the CI commands; the Apothicaire build line. |
| `caladrius/AGENTS.md` | 181 lines (kept under 200) | read on demand: `CLAUDE.md`, the role files and the resume prompt all point to it | The contract. It wins over any task. Its sections are listed below. |
| `caladrius/.claude/agents/<role>.md` | 7 files, 29 to 189 lines | as subagent types, only when the session starts in `caladrius/` | One role each. The front matter holds `name`, `description`, `model`, `effort` and `tools`. The body has the same plan in every file: You are / Motto / What you do / How / Report. `cargo xtask lint` checks that model and effort agree with `AGENTS.md` section 8. |
| `agent/CLAUDE.md` | 37 lines | sessions in `agent/` | Apothicaire notes: the gates, the sibling checkouts, the tests, PC-only work, frozen-set rules, a README without figures. There is no `AGENTS.md` there. |
| `optchat/CLAUDE.md` | 16 lines | sessions in `optchat/` | OptChat notes: what Apothicaire imports, the tests, the server, measured files never edited by hand. |
| umbrella `C:\Users\abdou\apothicaire\` | none | — | It has no `CLAUDE.md`, and its `.claude/` holds only a lock file. A session opened there does not see the role files. |
| `~/.claude/` (user level) | — | every session | `settings.json`: status line, auto mode, an auto-compact setting, workflows enabled. `commands/plume.md` is the Plume writing skill. There are no user-level agents. Claude's auto-memory sits under `~/.claude/projects/`: an index plus one file per fact, private and outside every repository. |

The sections of `AGENTS.md`:
1. Scope, with the order of the bricks.
2. What the human does (a closed list: answer `QUESTIONS.md`, decide every publication).
3. Ten golden rules: clean room, tests are the gate, engine first, everything is a command, nothing baked in, never crash, thin UI, look at what you draw, never break wasm, same numbers on every machine.
4. The repository map and its layers.
5. Oracle and tolerances.
6. Where a specification comes from.
7. UI.
8. The agent organization (the roles table).
9. The board, and one session = one iteration.
10. Budget.
11. Before you finish a task.

## 2. Roles (`AGENTS.md` section 8)

| Role | Model, effort | Owns |
|---|---|---|
| orchestrator | Opus, max | the main session: plans, writes cards, delegates, integrates, re-runs the checks, keeps the board, talks to Abdou in French; writes no code |
| engine | Opus, high | `caladrius-nca`, `caladrius-models`, `caladrius-fit` (every number) |
| reader | Sonnet, high | `specs/`, from open sources, with a status tag on each rule |
| oracle | Sonnet, high | `oracle/`, `caladrius-testkit`, the R scripts, the tests that fail first |
| interface | Sonnet, high | workspace, `xtask`, `caladrius-project`, engine commands, UI, CLI, MCP, web |
| reviewer | Fable, high | reads a diff and its card that it did not see written; the iteration audit |
| janitor | Haiku, low | mechanical housekeeping, no decision |
| onizuka | Opus, high | explains in French; writes the learning page of each iteration |

The separations that matter:
- Whoever writes an oracle test does not write the code under test.
- Every change in the three numerical crates is reviewed by `reviewer` (Fable) before it is `done`. Other cards may get a Sonnet reviewer.
- A subagent report has at most ten lines and ends with one bolder next step. It is a claim: the orchestrator re-runs the completion command before marking the card done.

## 3. The board (`board/`)

- `INDEX.md`: one row per card (id, title, role, state, depends on). Only the orchestrator writes it.
- `tasks/T-xxx.md`: one card per task. A card holds the goal, the allowed files, a completion criterion (a command and its expected result), the dependencies, the state, and a dated thread. States: `todo`, `in progress`, `to review`, `done`, `blocked`, plus `split` for an umbrella card. There is no `paused` state.
- `LOG.md`: append only, one entry per finished task.
- `QUESTIONS.md`: Q-xxx entries. Each holds the question, the decision taken meanwhile, and Abdou's answer quoted from chat.
- `RESUME.md`: the resume procedure, the state, what waits for Abdou, and the places.
- `messages/<date>-<from>-<to>-<subject>.md`: notes between agents, reviews and briefs. Agents never talk to each other directly.
- Commits start with the task id. The board is versioned and is the memory from one session to the next.

## 4. One iteration (`AGENTS.md` section 9)

One session is one iteration, and the iteration is itself a card (`- **Kind:** iteration`):
1. **Brief:** Goal, Deliverables, Claims touched, Done when, Plan (waves, cards, roles). Every card is written before it is assigned.
2. **Delegate:** the independent cards of a wave run in parallel, at most two subagents at a time and at most one on Fable.
3. **Verify:** the orchestrator integrates and runs section 11 on the whole tree.
4. **Audit:** `reviewer` reads the iteration's whole diff.
5. **Outcome:** results with their denominators, lessons, and a bolder next step that is proposed and never started.
6. **Teach:** `onizuka` writes a learning page in French, published as an Artifact.

Iterations so far:
- Iteration 1, T-124: READMEs that do not drift. Done.
- Iteration 2, T-130: the interface stack. Paused after wave 3.
- Iteration 3, T-139: the PK bricks. Paused on 2026-11-08.

## 5. Branches, worktrees and builds, as practised in iterations 2 and 3

- `main` receives only `docs/` and `board/` before 2026-11-18. Code lives on `iterN/<topic>` branches. Pushes are allowed by Q-013 (`main`), Q-019 (`iter2/*`) and Q-022 (`iter3/*`).
- One git worktree per branch, under `C:\Users\abdou\caladrius-iterN\<topic>`. Each agent has its own absolute `CARGO_TARGET_DIR` under `target/agent-<role>-<topic>`. Integration happens on one branch (`iter3/int`).
- Checks before a card is done: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo xtask layers | lint | review | wasm | conformance | digest`. CI runs them on Linux, Windows, macOS and wasm32.
- On this PC, programs under `target/` cannot write files (os error 5), so 25 tests fail for that reason alone. `xtask` binaries are copied outside `target/` to run.

## 6. What iteration 3 showed about this setup (observations, not proposals)

- The setup produced 30 reviewed or verified cards between 2026-11-07 and 2026-11-08. It also spent the usage budget Abdou had set, and it did so without warning before the limit.
- When the PC was shut down and the limit was hit, three agents died mid-card: T-154 (no report), the T-172 re-check, and T-174. The board kept everything needed to resume, but the work in their heads was lost.
- The many review rounds (a Fable review, a fix round, a Fable re-check) made each numerical card safer and dearer.
- Long tests held agents while they waited: the sealed population test takes about 530 to 760 s in release, and over 43 minutes in debug.
- The parallel layout left 20 worktrees and 85 build directories behind. They were cleaned on 2026-11-08 (`LOG.md`, Q-028).
- The orchestrator's own context filled several times in one iteration and had to be summarised.
- The role files load only when a session starts in `caladrius/`. A session opened in the umbrella sees none of them.
