# How to resume (written 2026-10-08, end of the first session)

1. Open Claude Code in `C:\Users\abdou\apothicaire\caladrius` (so that `.claude/agents/*.md` are loaded as agent types) and say: "Reprends le rôle d'orchestrateur Caladrius : lis AGENTS.md, board/INDEX.md, board/RESUME.md et les cartes `in progress`, puis continue."
2. Subagents do not survive a session: start new ones per role from the cards; give each `one` card, its allowed files, and an ABSOLUTE `CARGO_TARGET_DIR` under `target/agent-<role>`.
3. Current state: steps 1-4 reached; step 5 (UI) in progress: T-024 done, T-026 (fit page) in progress (see its card for the work-in-progress commit). Then T-027 (project save/load in UI, command palette, settings), step 6 (wasm demo, README conformance table), step 7 (two compartments, user models, several subjects).
4. Pending for the human: Q-012 (reference mini-campaign on Theoph subject 1), exercises 2-4 exports into `private/exports/td2..4/`, Windows decimal symbol for the reference software.
5. Publication: https://github.com/officina-dmpk/caladrius (Q-013 authorises pushes of this repo). Push after each closed task.
6. Apothicaire (the agent) lives in `C:\Users\abdou\apothicaire\agent\` (not a git repo yet), OptChat prototype in `..\optchat\`; see `..\README.md` (Officina umbrella).
