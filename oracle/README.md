# Public oracle

Versioned ground truth for the conformance tests (see `AGENTS.md` section 5).

- `data/`: public datasets as CSV with their origin and license in `data/README.md`.
- `scripts/`: R scripts that compute the expected results (PKNCA), with the options written explicitly and the package versions recorded.
- `expected/`: the generated expected results. Regenerate with the script; never edit by hand.

Private references (coursework exports, reference software outputs) never go here; they live in `private/`, which is git-ignored.
