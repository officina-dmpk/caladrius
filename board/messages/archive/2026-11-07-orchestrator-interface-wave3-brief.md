# Wave 3 brief: one NCA screen, three stacks (T-135, T-136, T-137)

From: orchestrator. To: interface. Iteration T-130.

The three prototypes build the same screen from this one brief. The comparison measures the stacks, so they must differ in nothing else.

## No visual direction yet

On 2026-11-07 Abdou turned down the three mockups of T-132 and will bring another direction later. The prototypes therefore take **no visual direction**:
- the layout is the one of the current egui NCA page (`docs/interface-inventory.md`, sections 9 and 11), placed plainly;
- every colour, radius and spacing comes from `crates/caladrius-ui/src/theme.json`, light and dark, read at build or run time and never copied by hand into code;
- one font family for text and one monospaced family for numbers, the stack's default or a system font. No web font and no icon set.

Do not invent a style. The chosen direction will be applied later to the winning stack only. Do not open `../notes/iteration-2-mockups/`.

## The screen

One window, data to results, for one NCA analysis.

1. **Top bar.** Dataset name, analysis label, subject selector, a Run button, and a `fresh` or `stale` badge.
2. **Data table**, left. Time, concentration, and the role of each point as a chip. The roles come from the NCA scene of `plot.scene` (observed, BLQ, Tmax, λz, excluded…), never worked out in the front. A cell can be edited: the analysis then shows `stale` until it runs again (UX-FR-12).
3. **Plot**, centre. It is drawn from the NCA scene of `plot.scene` and nothing else: no chart library and no WebGL, and the λz line comes from the scene. It has a semi-log and linear toggle and the note on the points that a log axis hides (UX-LOG-02). The λz range is chosen on the plot in three ways: drag either of two handles, click a point to include or exclude it, or pick a row in the λz candidates list. Each change calls `nca.run` again with the same parameters the egui page sends today (`crates/caladrius-ui/src/nca.rs`), and there is a "Back to automatic" action.
4. **Results**, below. The headline parameters with their units and flags, with the rest under "All parameters". Every number goes through `caladrius-format` (via `caladrius-bridge` in a web front): no front formats a number its own way. Copying the table gives tab-separated text with a header row, and the digits are those shown.
5. **Seal line**, under the results. The seal of the last `nca.run` (short form), the engine version and the command id, all from the answer.
6. **Keyboard.** Every action of steps 1 to 5 is reachable without a mouse, including moving each λz handle point by point. The card thread lists the keys.
7. **Never crash** (rule 6): an engine error is shown as its sentence, in place.

Data: `oracle/data/edge_blq.csv`, subject 3 on start, extravascular, dose 100; and `prototypes/nca50/nca50.csv` (50 subjects of 20 points, made by `prototypes/nca50/make.sh`) for the measure.

## Engine path

- The egui prototype calls the engine in process, through `caladrius-bridge`'s Rust type or `Engine::execute`.
- The web prototypes call it through the TypeScript client of `apps/caladrius-bridge/ts/`: `WorkerTransport` in a browser and `TauriTransport` in the Tauri shell. The same build runs in both.

## Measure mode (the same contract for all three)

Wave 4 measures the three with one script, so every prototype has the same measure mode.
- **Start.** Desktop: the variable `CALADRIUS_MEASURE=<path of a JSON file>`. Browser: the query `?measure=1`, which prints the JSON as one console line starting with `CALADRIUS_MEASURE `.
- **Run.**
  1. Load `nca50.csv`.
  2. Make 100 drags: drag k (0 to 99) takes subject `1 + k % 50` and sets the λz start to the k-th entry of a fixed cycle over the last 3, 4, 5 and 6 points.
  3. Each drag goes through the same handler a pointer drag uses, not through operating-system input, and waits for the frame that shows the new result.
  4. Then compare every result cell on screen with the formatted value of the answer.
  5. Wait 3 seconds (wave 4 samples the memory then) and exit. A browser page stays open.
- **The JSON**, with exactly these keys:

```json
{"stack": "egui|react|leptos", "version": "<git describe>", "first_frame_ms": 0.0,
 "drags": 100, "input_to_paint_ms": [], "frame_ms": [],
 "cells_checked": 0, "cells_equal": 0, "errors": []}
```

  `first_frame_ms` is counted from process start (desktop) or from `performance.timeOrigin` (browser).

## Delivery, for each prototype

- **One launch command**, in a short `README.md` in the prototype folder: one for the window, plus one for the browser when the stack has a browser build.
- **Screens.** PNGs in `prototypes/<stack>/screens/` at 1440×900, light and dark, fresh and stale, rendered without anyone at the screen:
  - egui: an example like `caladrius-ui`'s snapshot;
  - web: headless Edge (`msedge --headless --screenshot`) on the built page.
  Look at every PNG before you report (rule 8).
- **Measure file.** One measure run on this PC, committed as `prototypes/<stack>/measure-local.json`.
- **Work log** in the card thread: the corrections after the first version that ran, the dependencies added with their licences, the size of the release build, and anything that did not work.

## This PC

- Programs under `target/` are refused file writes (os error 5). Keep measure output and screenshots outside `target/` (the prototype folder).
- Build first with plain `cargo` before any `cargo xtask` command that links (LNK1104 otherwise).
- Absolute `CARGO_TARGET_DIR` under `C:\Users\abdou\apothicaire\caladrius\target\agent-interface-<stack>`.
