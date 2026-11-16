# Is Leptos 0.8 + Tauri 2 the right stack? An independent investigation (T-130)

From: reviewer (Fable). To: orchestrator, Abdou. Date: 2026-11-07. Read-only: no code changed, nothing pushed.

## Verdict

**Keep Leptos, under three conditions. Confidence: moderate (about 65 %).**
1. The engine leaves the UI thread before any fit ships: native through Tauri commands, wasm in a Web Worker in the browser, both behind the one bridge contract.
2. The bridge gains a progress and cancel protocol; `execute(request) -> sealed answer` alone cannot carry a population fit.
3. Leptos's maintenance status is re-read at each release cycle; Dioxus stays the named fallback and nothing Leptos-specific is written outside the view layer.
Nothing found refutes the choice. Nothing in wave 3 proves it either.

## What wave 3 proves, and does not

**Proved (measured here, this PC, 2026-11-07).** Three stacks render the same screen from the same scene and show the same seal `eb5570976fcb`. Each prototype's tests pass again: egui 3, Leptos 11, React 16. The engine's cost for one drag is small and the same in wasm and native: `nca.run` p50 3.1 to 3.2 ms native and 3.4 to 3.7 ms in wasm (Node 22), `plot.scene` 1.5 ms in both, so about 5 ms per drag (scratchpad bench, 100 drags, the measure cycle). Wasm costs nothing measurable over native for NCA on 20 points.

**Not proved: that Leptos is twice as fast as React.** The wave 3 ratio (7.1 against 13.4 ms p50) is dominated by the engine path and the environment, not by the framework.
- About 5 of Leptos's 7.1 ms are the engine itself, called synchronously in the same wasm. React pays two sequential round trips per drag (`nca.run`, then `plot.scene`, in `session.ts` `runOnce`) through a worker or Tauri IPC, each carrying 5 to 7 kB of JSON, plus the worker's own engine time. React in the browser (13.1 ms, worker) equals React in Tauri (13.4 ms, IPC): the cost is the two round trips, not the transport.
- Under identical conditions the difference vanishes. I ran both browser builds in headless Edge (same binary, `--disable-gpu`, 1440×900, alternating, twice each):

| stack | input to paint p50 / p95 / max (ms) | first frame (ms) | cells |
|---|---|---|---|
| Leptos, runs 1 and 2 | 15.5 / 17.6 / 18.1 and 15.3 / 18.0 / 18.9 | 149, 156 | 151/151 |
| React, runs 1 and 2 | 15.2 / 17.3 / 28.5 and 15.2 / 17.7 / 27.3 | 260, 247 | 151/151 |

Both sit on one display frame (about 16.7 ms at 60 Hz): the measure's resolution is one vsync, and a 5 ms engine plus either framework fits inside it. React's `frame_ms` p50 is 33 ms against 15 for Leptos, the only trace of its two round trips: it spans two vsyncs where Leptos spans one.
- The wave 3 figures below 16.7 ms mean that in the Tauri window `requestAnimationFrame` was not locked to the display. "Input to paint" there measured a rAF callback plus a MessageChannel hop, not a presented frame. It is a fair latency proxy but not evidence that a frame was shown, and it depends on whether the window is visible: hidden in the app's browser pane, Leptos gave 12.3 ms p50 with frames quantized at 12.3 ms.
- egui's 5.19 ms is CPU time to the end of `App::ui` without presentation (T-135), a third definition. The three columns are not comparable yet, as the card already says.

**What wave 3 does show in Leptos's favour.** Smaller JS (53 kB against 172 kB gzipped), one language and one type system from engine to screen, and `caladrius-format` reached by a Rust call rather than a second wasm instance loaded beside React. The wasm is 3.0 MB with the engine inside against 1.8 MB; first frame is equal in Tauri (538 against 554 ms) and better in the headless browser (about 150 against 250 ms).

## Recommended architecture for population PK

The question that decides the stack is where the engine runs during a fit of minutes, not during a 5 ms drag.

| placement | compute | OFV per iteration | cancel | memory | browser without Tauri |
|---|---|---|---|---|---|
| A. wasm on the UI thread (Leptos today) | one core, window frozen | impossible without yielding | impossible | wasm32, cannot shrink | yes |
| B. wasm in a Web Worker | one core, UI free | `postMessage` per iteration | `Worker.terminate()` (loses state; re-instantiate, about 19 ms import) or a cooperative flag checked between iterations | same, per worker | yes |
| C. wasm threads (SharedArrayBuffer, COOP/COEP, wasm-bindgen-rayon) | all cores | shared memory | atomic flag | shared, cannot shrink | only with the headers; WebKitGTK ships SAB off |
| D. native in the Tauri process, `#[tauri::command(async)]` + rayon | all cores, native speed | `tauri::ipc::Channel` per iteration | `AtomicBool` in managed state | native | no |

**Recommendation: D for the desktop, B for the browser, one bridge in front of both.** The bridge is already the seam (T-134: `TauriTransport`, `WorkerTransport`, bytes identical to `Engine::execute`). What is missing is stack-independent: a long-running command shape (`fit.start` returning a job id, progress events with iteration, OFV and parameters, `fit.cancel`, then the sealed answer at the end), and the engine checking a cancel flag between iterations (rule 6: a cancel is a sentence, never a panic). Progress events are unsealed; only the final answer is sealed.

C is rejected for now, not forever: wasm-bindgen-rayon 1.3.0 (2024-12-21, pre-2026) needs nightly with `build-std` and `+atomics`, against a toolchain pinned to stable 1.99.1; `std::thread::spawn` panics on `wasm32-unknown-unknown`; Tauri can set COOP/COEP since 2.1.0 (PR #11485, merged 2024-11-06, pre-2026) but I found no primary source confirming `crossOriginIsolated` is true inside a Tauri window on all three platforms, and WebKit bug 269208 (last modified 2026-01-18) still has SharedArrayBuffer off by default in WebKitGTK.

**What Leptos imposes.** Today's Leptos prototype links the engine into the same wasm and calls it from the handler (`session.rs`), which is A. Moving to B or D from Rust means a Rust transport: `web_sys::Worker` with serialized messages, and `invoke` through wasm-bindgen or a crate such as tauri-sys, with the answer awaited as a future and written into the signal. The TypeScript client of T-134 then goes unused by the app (the site may still use it). That is a day of code, but it is the one place where Leptos costs more than React, where the TS client already exists. Tauri's own documentation (calling-rust page, updated 2026-06-08) confirms the rest: a sync command runs on the main thread, `async` commands run on `async_runtime::spawn`, `Channel` is the recommended path for streaming, `ipc::Response` carries raw bytes. No official IPC latency figures exist; the anecdotal ones (Discord, 2024-12-09, pre-2026) are not used here.

## Risks, ranked, with sources

1. **Maintenance and bus factor: the one that could make Abdou regret the choice.** On 2026-05-08 the lead maintainer (gbj) opened and pinned issue #4707: Leptos is "not abandoned but will be lightly maintained", considered feature-complete, open to additional maintainers; volunteers answered from May to July 2026. Commits over the last 12 months: gbj 275, sabify 247, nobody else near; 110 open issues on 2026-11-07. Yet 0.8.23 and 0.9.0-rc1 shipped on 2026-10-31, so the project is alive. Reading: a two-person project in maintenance mode. Over three years this is the real risk: a security fix or a wasm-bindgen break could wait weeks. Mitigation: pin, vendor if needed, keep the view layer thin.
2. **API breaks.** 0.7.0 (2024-11-30, pre-2026) was a rewrite; 0.8.0 (2025-05-01, pre-2026) broke again; 0.9.0-rc1 (2026-10-31) says mostly no changes to migrate, with named breaks (`lazy` flag, serde_qs 1.0, `foo()` call syntax, `*signal`). About one major per year, each migration small to medium. T-137 already met one: a `rust-version` line made the resolver pick 0.8.3, which breaks against syn 3.
3. **Ecosystem thinness.** No mature docking or panel layout crate (dockview has no Leptos equivalent); tables: leptos-struct-table 0.19.1 (2026-09-02); charts: leptos-chartistry 0.2.3 (2026-01-23, idle); components: thaw 0.5.0-beta (2025-08, pre-2026); utilities: leptos-use 0.20.0 (2026-10-20). Caladrius draws `plot.scene` itself and formats through the engine, so charts and tables matter less than for most apps; a docking layout (the Rerun-style blueprint named in the card) would be written by hand or glued to a JS library through wasm-bindgen. Downloads: leptos about 1.68 M per 90 days on crates.io, react about 252 M per week on npm.
4. **Developer loop and debugging.** Compile times measured here on the Leptos prototype (wasm32, `CARGO_TARGET_DIR` fresh for this agent): a one-line change in app.rs rebuilds in 2.7 s in the dev profile and 58 s in the release profile (opt-level s, lto); a full wasm32 build is 58 s dev and 90 s release, proc macros already cached. The React prototype's `vite build` takes 2.2 s (3.1 s with tsc). The dev loops are close; a release check of a visual change costs a minute in Rust. Wasm debugging: breakpoints work with DWARF in Chromium, variable inspection does not (per the DWARF debugging tooling notes); a Rust panic in wasm shows a stack with no locals. The card itself says agents write React most reliably (T-130 brief, point 2); T-137's corrections were data-shape bugs, not Leptos bugs.
5. **Two bridge clients or one.** If the app is Leptos and the site React, the TS client and a Rust client both live. Decide now which the app uses.
6. **Not Leptos-specific, so not discriminating:** accessibility (real DOM, same as any web front, far better than egui's canvas); end-to-end tests (Playwright drives the DOM, stack-blind); Tauri on Linux (WebKitGTK blank windows and software WebGL, Tauri Linux graphics page 2026-06-15, issue #9394) hits React identically; CSP `wasm-unsafe-eval` already set.
7. **Contributors.** A Rust front raises the bar for pharmacometricians who write R or Python. Today the team is Abdou and agents, so this weighs little; it would weigh if the open research workshop draws outside contributors to the interface.

Not found despite searching: js-framework-benchmark rows for Leptos 0.8 against React 19 from a dated run; official Tauri IPC throughput figures; a dated confirmation of `crossOriginIsolated` in Tauri windows.

## Alternatives that matter

- **Dioxus 0.7.10** (stable; 0.8 alphas carry breaking changes; Dioxus Labs joined Cognition on 2026-09-10). Same signals-and-macro shape as Leptos, a funded team, a native renderer in progress. It is the fallback if #4707 turns into archiving. It does not change the decision today: 0.8 is unstable, and a migration from Leptos would be days, not weeks, if the view layer stays thin.
- **Solid 2.0.0-rc.21 or Svelte 5.57.3 with Tauri** would give Leptos's fine-grained updates with React's ecosystem (dockview 8.8.3 has a vanilla core) and the existing TS client. Worth it only if the Rust dev loop proves too slow for the team; nothing measured says so yet.
- **Slint 1.18.1** (2026-09-21; web target not recommended by its authors, canvas only, no accessibility tree), **Iced 0.14.0** (2025-12-07, pre-2026; experimental web, no release since), **GPUI 0.2.2** (no web): all fail the browser-without-Tauri requirement.
- **egui kept, with egui_tiles 0.18.0** (egui 0.37.0, 2026-10-15). Rerun proves egui in wasm scales; egui_plot 0.38.0 still has no log scale (issue #11, PR #244 open) but Caladrius draws its own scene. Weak accessibility on the web and Vulkan-only on this PC today (T-135) keep it a fallback, not a candidate.

## Wave 4 protocol

Scenarios, each writing the wave 3 JSON keys plus `visibility_state`, `raf_interval_ms` (median of 30 idle frames, so a throttled run flags itself) and `env` (tauri or browser, GPU on or off):
- **S1 drag**, as today, 100 drags, but "paint" means the same thing in all three: web keeps rAF + MessageChannel; egui adds presentation (time after `present`) or is marked "CPU only" in the table.
- **S2 heavy plot**: 50 subjects overlaid, about 20,000 points, VPC bands from `plot.scene`, 100 pan or zoom steps.
- **S3 fit**: one popPK dataset by `model.simulate` (command line committed), one fixed fit of N iterations. Measure wall time, OFV events received per second, UI `frame_ms` during the fit (a dummy interaction every 100 ms), cancel latency from request to stopped, memory after one fit and after ten.
- **S4 cold start and size**, unchanged.

Conditions: both web fronts in the same Tauri window type, visible and foregrounded, same monitor; three runs alternating stacks, first run discarded; p50 and p95 with n. Headless Edge as a second environment for the browser builds.

Thresholds that change the verdict: on S1 and S2, p50 within one frame (16.7 ms) and p95 within 1.5 frames means equivalent; a stack more than 1.5 times slower at p95 on S2 loses. On S3, any long task over 50 ms on the UI thread during a fit fails (web.dev long task, pre-2026 definition); cancel over 200 ms fails (INP threshold); memory after ten fits above twice the peak of one fails (wasm memory cannot shrink).

Prepare now, before popPK exists: the bridge's start, progress and cancel shape as a spec; the Rust transport for Leptos (B and D); a measure runner that launches both Tauri shells with `CALADRIUS_MEASURE` and collects the JSON; the `visibility_state` and `raf_interval_ms` keys; egui's presentation timing.

## What would change the verdict

- To **change**: issue #4707 moves to archived, or no release for 12 months; 0.9 final ships breaks larger than its rc notes; the Rust transport for B and D costs more than a week; wave 4 S2 shows Leptos more than 1.5 times slower than React at p95; a layout need only a JS docking library meets, and gluing fails.
- To **keep without conditions**: wave 4 S3 passes on D and B with the bridge protocol in place; 0.9 final lands with a small migration; a third maintainer appears on #4707.

## Sources

Measured here (2026-11-07, this PC): wave 3 measure files (T-135 to T-137 cards); scratchpad engine bench, native and Node 22 wasm; headless Edge runs above; prototype test runs; build timings above.

Published, with dates:
- Leptos issue #4707 (gbj, 2026-05-08, open, pinned); releases 0.7.0 (2024-11-30, pre-2026), 0.8.0 (2025-05-01, pre-2026), 0.8.23 and 0.9.0-rc1 (2026-10-31); GitHub contributor counts and 110 open issues (read 2026-11-07).
- leptos-struct-table 0.19.1 (2026-09-02); leptos-chartistry 0.2.3 (2026-01-23); leptos-use 0.20.0 (2026-10-20); thaw 0.5.0-beta (2025-08, pre-2026).
- Tauri 2 "Calling Rust from the frontend" (updated 2026-06-08); COOP/COEP headers PR #11485 (merged 2024-11-06, Tauri 2.1.0, pre-2026); Linux graphics page (2026-06-15, issue #9394).
- WebKit bug 269208, SharedArrayBuffer in WebKitGTK (modified 2026-01-18); wasm-bindgen-rayon 1.3.0 (2024-12-21, pre-2026); `std::thread::spawn` on wasm32-unknown-unknown panics (Rust std docs).
- web.dev long tasks 50 ms and INP 200 ms (pre-2026 definitions).
- Dioxus 0.7.10 and 0.8 alphas; Dioxus Labs and Cognition (2026-09-10). Slint 1.18.1 (2026-09-21). Iced 0.14.0 (2025-12-07, pre-2026). GPUI 0.2.2. Svelte 5.57.3, Solid 2.0.0-rc.21, dockview 8.8.3 (read 2026-11-07).
- egui 0.37.0 (2026-10-15); egui_plot 0.38.0, issue #11 and PR #244; egui_tiles 0.18.0.
- Anecdotal, not used for any figure: Tauri IPC sizes on Discord (2024-12-09, pre-2026).
