# How to resume (written 2026-10-08, end of the first session)

1. Open Claude Code in `C:\Users\abdou\apothicaire\caladrius` (so that `.claude/agents/*.md` are loaded as agent types) and say: "Reprends le rôle d'orchestrateur Caladrius : lis AGENTS.md, board/INDEX.md, board/RESUME.md et les cartes `in progress`, puis continue."
2. Subagents do not survive a session: start new ones per role from the cards; give each `one` card, its allowed files, and an ABSOLUTE `CARGO_TARGET_DIR` under `target/agent-<role>`.
3. Current state: steps 1-5 reached (UI: T-024, T-026, T-027 done). Next, per the two independent reviews of 2026-10-08 (../notes/review-*.md, pending the human's formal yes): tag v0.1.0 with CI, CITATION.cff and an honest README (coverage table per model, tolerance note); deterministic number gate and digest tests in the agent; benchmark of simulated exercises; two compartments; then UI polish, wasm, decision models.
4. Pending for the human: Q-012 (reference mini-campaign on Theoph subject 1), exercises 2-4 exports into `private/exports/td2..4/`, Windows decimal symbol for the reference software.
5. Publication: https://github.com/officina-dmpk/caladrius (Q-013 authorises pushes of this repo). Push after each closed task.
6. Apothicaire (the agent) lives in `C:\Users\abdou\apothicaire\agent\` (not a git repo yet), OptChat prototype in `..\optchat\`; see `..\README.md` (Officina umbrella).
