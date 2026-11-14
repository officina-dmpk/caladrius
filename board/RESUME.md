# How to resume (state of 2026-11-14; the sections below "Demo" are from 2026-11-08)

## Demo of 2026-11-16 (written 2026-11-14)

- **Iteration 3 resumed for the demo.** T-139's thread has the order and the state.
- **Worktrees:** under `C:\Users\abdou\caladrius-iter3\`: `int`, `pop`, `pop-triage` and `front`. Data outside the repository is in `C:\Users\abdou\caladrius-iter3\data\`: the engine CSV of T-154, the R triage cache, and the reviewer's harness.
- **The front for the demo** is on `front/int` at e3c7a3e.
  - Browser:
    1. From `apps/`, run `npm ci`.
    2. Run `npm run wasm -w @caladrius/front` with `CARGO_TARGET_DIR` set.
    3. Run `npm run build -w @caladrius/front`.
    4. In `apps/caladrius-front`, run `npx vite preview --host 127.0.0.1 --port 4173`.
  - Tauri window: `C:\Users\abdou\caladrius-iter3\bin\caladrius-tauri.exe`.
  - The pk3 sample needs initial estimates typed by hand: CL 2, Vc 10, Q2 5, Vp2 20, Q3 0.5, Vp3 40.
- **The demo uses the new design** (Abdou, 2026-11-14): T-187, designed by Fable, on branch `claude/front-design-879a0b`, worktree `.claude/worktrees/front-design-879a0b`.
  - Browser: port 4174, served from that worktree.
  - Window: `C:\Users\abdou\caladrius-t187\bin\caladrius-tauri.exe`.
  - The old front of `front/int` is the fallback.
  - Merging the design into `front/int` (PR #22) is for Abdou to decide.
- **Population PK:** no `pop.*` command exists (T-156 is not written), and `front/int` is built from `iter3/int`, not `iter3/pop`, so the front shows no population fit.
  - T-154 is verified. Its Fable review requests changes: two blockers, no wrong estimate (Q-029). No engine change was made before the demo.
  - T-174 is done, with verdict (a): the cl bias belongs to the POP-OBJ-02 objective, not to the engine (Q-030).
  - T-188 is to review: 44 oracle tests, of which 6 fail on spec rules for the engine fix round. `POP_SE` is proposed in Q-026.
  - The demo calls the estimation "in validation". A rehearsable command in the `pop` worktree runs 135 tests of the objective and the exact fits in under a second, once built: `cargo test --release -p caladrius-fit --test oracle_pop -- ofv_ fit_exact`.
- **Card ids:** `front/int` numbers its own cards T-177 to T-185, which collide with those of `main`. New cards start at T-186.
- **Not pushed:**
  - `main` (board);
  - `iter3/int` f91891a;
  - `iter3/pop` 9f93d0f;
  - `iter3/pop-triage` 363dadf;
  - `front/int` e3c7a3e (local branch, ahead of `origin`);
  - `claude/front-design-879a0b`, the `front-design` session's branch.
- **Waiting for Abdou:** Q-026 (`POP_SE`), Q-029, Q-030; what the demo shows of Apothicaire; whether the engine fix round of T-154 and T-156 (`pop.fit`) come after the demo; the renumbering of the colliding card ids.

**Before 2026-11-14, the project was paused.** The usage budget for the iterations was spent, and Abdou was changing how the project is run. The steps below describe the setup as it was on 2026-11-08.

1. Open Claude Code in `C:\Users\abdou\officina\caladrius`: the agent files of `.claude/agents/` load only when a session starts here, and `CLAUDE.md` imports `AGENTS.md`. Say: "Reprends le rôle d'orchestrateur : lis board/RESUME.md, board/INDEX.md et la carte d'itération ouverte, puis continue."
2. One session = one iteration (`AGENTS.md` section 9). The open iteration card holds the brief, the plan and the state of each card.
3. Subagents do not survive a session. Start one per card with its role; the brief gives the card, its allowed files, and an absolute `CARGO_TARGET_DIR` under `target/agent-<role>`.
4. A subagent's report is a claim: re-run the card's completion command before marking it done. Subagents do not commit; the orchestrator does (T-177).
5. No worktree exists any more. To work on a branch, create one again with `git worktree add C:\Users\abdou\caladrius-iter3\<name> iter3/<branch>`.

## State

- **Iteration 3, T-139: paused** (`blocked`) on 2026-11-08. It covers the missing PK bricks of the engine: three compartments, the individual ML fit, population PK with FOCEI, and population simulation.
  - 30 cards are `done`. Brick 1 (pk3) and brick 2 (ML fit) are complete and merged on `iter3/int`. Brick 4 (the simulator) is complete on `iter3/sim`.
  - Brick 3 (population): the objective (T-153) is done and reviewed. The estimation (T-154) is written but not verified.
  - **T-154** `to review`: code on `iter3/pop` (up to 712ad97), with no agent report, no verify and no review. The cl bias of design A is not triaged. Its thread says how to resume.
  - **T-172** `to review`: the integration on `iter3/int` (fix 907b283, then the T-175 merge ea98dd2). It waits for the Fable re-check, after a one-line fix of a duplicate "## 14" heading in `specs/sources.md`.
  - **T-174** `todo`: the triage of the cl bias in R and saemix. Unfinished scripts are on `iter3/pop-triage` (963dbd1). It waits for the verify of T-154.
  - **T-176** `todo`: the published `fit.run` schema lacks the ML options. It comes after T-172.
  - **Not written yet:** T-155 (CWRES; 40 `oracle_pop` tests wait for it), T-156 (`pop.fit` as a long-running command), T-161 (the wave 4 datasets), the `specs/population.md` round, the merges of `iter3/pop` and `iter3/sim` into `iter3/int`, and un-ignoring SIM-ORA-06.
  - **Estimate** given to Abdou on 2026-11-08: 12 to 21 hours of agent work to close the iteration. This is an estimate, not a measure.
- **Branches of iteration 3**, all pushed (Q-022):

  | Branch | What it holds |
  |---|---|
  | `iter3/int` | bricks 1 and 2 (T-172, T-175) |
  | `iter3/pop` | brick 3 so far (T-152, T-153, T-154) |
  | `iter3/pop-ora` | T-173, not merged into `iter3/pop` yet |
  | `iter3/pop-triage` | T-174 work in progress |
  | `iter3/sim` | brick 4 (T-157 to T-160) |

  The eleven other `iter3/*` branches were deleted on 2026-11-08, because each one is contained in `iter3/int` or `iter3/pop`. Their tips are in `LOG.md`.
- **Iteration 2, T-130: paused** (`blocked`) until population PK with an OFV is in the engine. Waves 1 to 3 are done. Left: wave 4, T-138, the audit and the outcome. Wave 3 has no visual direction, since Abdou turned down the three mockups; Leptos is his favourite so far.
  - Branches, pushed under Q-019: `iter2/base` (T-133 and T-134), `iter2/egui`, `iter2/react` and `iter2/leptos`.
  - `iter2/scene` and `iter2/bridge` were deleted, since `iter2/base` contains them.
  - `iter2/base` adds a CI push trigger for `iter2/**`. Remove it before merging into `main`.
- Only `docs/` and `board/` reach `main` before 2026-11-18.
- **First deadline:** the Sanofi demo of 2026-11-17 stays on the current egui interface of `main`.

## This PC

- Programs under `target/` are refused file writes (os error 5). `cargo test --workspace --no-fail-fast` gives 25 failures for that reason alone. The linker started by `cargo xtask` can hit LNK1104 on a cold target, so build first with plain `cargo`.
- `cargo xtask digest` and `lint` are run from a copy of the xtask binary outside `target/`.
- On 2026-11-08 the agent build directories under `target/` were deleted. Kept: `target/human-release` (a release build of the application and the CLI), `target/snapshots` (the UI screenshots), and `target/agent-apothicaire`: `caladrius-mcp` built from `main` f8d1c1a, sha256 b72beca797749eb3f5abf78c0f5ec10d7188e2fbebfeeb0da25610695cd0e678. The frozen-set freeze hashes this binary. Every other build starts cold.
- Frozen set, case freeze: it waits for `../agent/bench/frozen/demo/wordings.json`, written by Abdou (2 per turn kind) and dsh (1 per kind) from `wordings.template.json`. Claude never opens that file. Then follow the "Commands" section of `../agent/bench/frozen/README.md`, with C1 = v1 and C2 = v2.

## Waiting for Abdou

- **The new way of running the project.** It is to be imported from the ols-prime setup in a separate session. The description of the setup as it stands is in `board/messages/2026-11-08-how-the-project-is-run.md`.
- **Iteration 3:**
  - whether to resume it, and with what budget;
  - Q-020 points 1, 2 and 6 (the interface lines of `AGENTS.md` and of the README), after the stack is retested on population PK;
  - Q-021: no Rtools, nlmixr2 or other R package is installed;
  - Q-023 to Q-027.
- **Iteration 2:** a new visual direction.
- Ideas that wait for Abdou:
  - the Fisher-information design, which needs an amendment of `AGENTS.md` section 1;
  - `qc.nca`;
  - the plan-first assistant;
  - the PAGE 2027 abstract.
- Q-004 (public datasets) is still open.
- Todo cards from the red team (T-054 to T-093, `INDEX.md`): most touch `../agent`, `../optchat` or the umbrella.

## Places

- Publication: https://github.com/abdou-rgd/caladrius (public). Q-013 authorises pushes of this repository.
- Apothicaire is `..\agent\` (github.com/abdou-rgd/apothicaire), OptChat is `..\optchat\` (github.com/abdou-rgd/optchat). The umbrella is `..\` (`README.md`, `notes/`); its repository github.com/abdou-rgd/officina is checked out in `C:\Users\abdou\officina-umbrella`.
