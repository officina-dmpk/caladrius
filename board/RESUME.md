# How to resume (state of 2026-11-16, after the demo)

The demo of 2026-11-16 is done; it ran on the new design (T-187). Abdou received feedback, not yet written on the board. The cleanup T-190 (Q-032) followed. Nothing is in progress.

## Start a session

1. Open Claude Code in `C:\Users\abdou\officina\caladrius`. `CLAUDE.md` imports `AGENTS.md`; the agent files of `.claude/agents/` load only when a session starts here.
2. One session = one iteration (`AGENTS.md` section 9). Read this page, `INDEX.md` and the open iteration card.
3. Subagents get one card, its allowed files and an absolute `CARGO_TARGET_DIR` under `target/agent-<role>`. They do not commit; the orchestrator re-runs the completion command, gets the review, commits.

## Iterations

- **Iteration 3, T-139** (`blocked`): the missing PK bricks of the engine. Paused after the demo, until Abdou decides what comes next.
  - Bricks 1 (pk3) and 2 (ML fit) are on `iter3/int`; brick 4 (the simulator) on `iter3/sim`.
  - Brick 3 (population): the objective T-153 is done. The estimation **T-154** is `to review`: verified, the Fable review requests changes (two blockers, no wrong estimate, Q-029). **T-188** is `to review`: 44 oracle tests on `iter3/pop`, 6 fail on spec rules the engine does not meet yet. T-174 is done: the cl bias belongs to the POP-OBJ-02 objective, not to the engine (Q-030).
  - **T-176** `todo`: the published `fit.run` schema lacks the ML options.
  - Not written: T-155 (CWRES), T-156 (`pop.fit` as a command), T-161 (wave 4 datasets), the merges of `iter3/pop` and `iter3/sim` into `iter3/int`.
- **Iteration 2, T-130** (`blocked`): the interface stack. **Decided 2026-11-16 (Abdou): egui stays the front**; the web/Tauri front is dropped and PR #22 is closed without merging.

## Branches and worktrees

| Branch | Where | What it holds | Pushed |
|---|---|---|---|
| `main` | this checkout | the board | yes |
| `iter3/int` | `C:\Users\abdou\caladrius-iter3\int` | bricks 1 and 2 | yes |
| `iter3/pop` | `...\caladrius-iter3\pop` | brick 3 so far, T-188 | yes |
| `iter3/pop-triage` | `...\caladrius-iter3\pop-triage` | T-174 | yes |
| `iter3/pop-ora`, `iter3/sim` | none | T-173 (contained in `iter3/pop`); brick 4 | yes |
| `front/int` | `...\caladrius-iter3\front` | the previous front (T-177 to T-186 of its own numbering); PR #22 closed without merging (egui kept) | 2 commits ahead |
| `demo/egui-pop` | `...\caladrius-iter3\demo` | the egui demo with population fit (contains the deleted `demo/pop-fit`, `demo/pop-ui`, `demo/ui-quick`); the way forward for the egui front. Its nine demo commits have no card yet; ultrareview of 2026-11-15: 4 of 9 findings fixed, 5 open in T-191 | yes |
| `claude/front-design-879a0b` | `.claude\worktrees\front-design-879a0b` | T-187, the new design used in the demo, not merged into `front/int` | no |
| `claude/objective-bardeen-af9f71` | `.claude\worktrees\objective-bardeen-af9f71` | T-189 (Q-031, golden rule 1), **not merged into `main`** | no |
| `iter2/*` | none | the stack prototypes of iteration 2 | yes |

- Two detached worktrees build the window: `C:\Users\abdou\caladrius-t187\tauri-src` (the demo window, binary in `caladrius-t187\bin\caladrius-tauri.exe`) and `caladrius-t189\tauri-src` (one uncommitted change in `tauri.conf.json`).
- **Card ids:** `front/int` and the T-187 branch number their own cards T-177 to T-185, which collide with `main`'s. T-186 to T-189 live on branches; `main` continues at T-192.
- Data outside the repository: `C:\Users\abdou\caladrius-iter3\data\` (the engine CSV of T-154, the R triage cache, the reviewer's harness).

## This PC

- Programs under `target/` can be refused file writes (os error 5). The linker started by `cargo xtask` can hit LNK1104 on a cold target: build first with plain `cargo`.
- T-190 deleted the build directories of every worktree: every build there starts cold. Kept in this checkout: `target/agent-apothicaire` (the `caladrius-mcp` that Apothicaire starts; the frozen-set freeze hashes it), `target/human-release`, `target/snapshots`.
- Frozen set, case freeze: it waits for `../agent/bench/frozen/demo/wordings.json`, written by Abdou and dsh from `wordings.template.json`. Claude never opens that file.

## Waiting for Abdou

- The demo feedback, to write on the board and turn into the next iteration.
- From the T-190 proposal: merge T-189 into `main`; push `front/int` and the `claude/*` branches (Q-013 covers pushes of this repository); `iter2/*` kept as branches or turned into tags.
- Open questions: Q-002, Q-004 to Q-012 (from 2026-10-08, mostly applied in practice, never answered), Q-023 to Q-027, Q-029, Q-030.
- Whether the T-154 fix round and T-156 (`pop.fit`) come next.
- Red-team todo cards (T-054 to T-093 in `INDEX.md`): most touch `../agent`, `../optchat` or the umbrella.

## Places

- Publication: https://github.com/abdou-rgd/caladrius (public). Q-013 authorises pushes of this repository.
- Apothicaire is `..\agent\` (github.com/abdou-rgd/apothicaire; worktree `..\agent-pk` on `demo/pk-tools`), OptChat is `..\optchat\` (github.com/abdou-rgd/optchat, 2 commits ahead). The umbrella is `..\` (`README.md`, `notes/`); its repository github.com/abdou-rgd/officina is checked out in `C:\Users\abdou\officina-umbrella`.
