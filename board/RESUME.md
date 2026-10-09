# How to resume (written 2026-10-08, end of the first session)

1. Open Claude Code in `C:\Users\abdou\apothicaire\caladrius` (so that `.claude/agents/*.md` are loaded as agent types) and say: "Reprends le rôle d'orchestrateur Caladrius : lis AGENTS.md, board/INDEX.md, board/RESUME.md et les cartes `in progress`, puis continue."
2. Subagents do not survive a session: start new ones per role from the cards; give each `one` card, its allowed files, and an ABSOLUTE `CARGO_TARGET_DIR` under `target/agent-<role>`.
3. Current state: v0.1.0 tagged; step 7 first item done on the engine side (T-031 specs, T-032 oracle, T-033 pk2 models, T-034a pk2 via engine/CLI/MCP, T-035 spec sync); T-034b (pk2 in the UI) in progress. Apothicaire (../agent, own git repo): deterministic number gate and digest tests done, 25-exercise benchmark running (bench/). Open for the human: Q-004..Q-007, Q-009, Q-012, Q-014.
4. Pending for the human: Q-012 (reference mini-campaign on Theoph subject 1), exercises 2-4 exports into `private/exports/td2..4/`, Windows decimal symbol for the reference software.
5. Publication: https://github.com/officina-dmpk/caladrius (Q-013 authorises pushes of this repo). Push after each closed task.
6. Apothicaire (the agent) lives in `C:\Users\abdou\apothicaire\agent\` (not a git repo yet), OptChat prototype in `..\optchat\`; see `..\README.md` (Officina umbrella).
