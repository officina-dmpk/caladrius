# Caladrius

Open-source pharmacokinetic analysis in Rust: non-compartmental analysis, individual compartmental models, weighted least-squares fitting and plots, with a native desktop UI (egui) and a WebAssembly demo. Every analysis is a command with a stable id, so the same calculations are available from the UI, the command line and an MCP server for agents. Caladrius is the calculation layer of Apothicaire, a local DMPK assistant.

Status (2026-10-08): steps 1 to 4 of the marching order are done: NCA, one-compartment models and weighted least-squares fitting pass 100% of the public oracle (`docs/conformance.md`: 3744 NCA values, 886 model values, 5340 fit values), a CLI and an MCP server expose every command, and the first private comparison with the reference software (one coursework exercise, two AUC methods) agreed on every value. The desktop UI (step 5) is in progress. See `AGENTS.md` for the contract and `board/INDEX.md` for the task board.

Named after the caladrius, the white bird of Roman legend said to take a sick person's illness away as it flies off.

## Trademark notice

Caladrius is an independent project and is not affiliated with, endorsed by or derived from Certara. Phoenix and WinNonlin are trademarks of Certara. Caladrius aims at compatible conventions for standard pharmacokinetic calculations, reimplemented from public textbooks and documentation.

## License

MIT OR Apache-2.0. Third-party data and assets are listed in `ATTRIBUTION.md`.

## Build

Requires Rust stable 1.85 or newer, installed with rustup (`rust-toolchain.toml` selects the stable channel). The WebAssembly check also needs the `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask layers   # prints the layer table, fails if a crate depends on a higher layer
cargo xtask wasm     # checks that layers L0 to L3 compile for wasm32-unknown-unknown
```

`cargo xtask` is an alias for the `xtask` crate (see `.cargo/config.toml`). The layers and their rules are in `AGENTS.md`, section 4.

## Conformance

`cargo xtask conformance` runs `caladrius-nca` on every case of `oracle/expected/`, `caladrius-models` on every case of `oracle/expected/models/` (relative error 1e-12) and `caladrius-fit` on every case of `oracle/expected/fit/` (1e-4 on estimates and statistics, 1e-6 on the residual sum of squares), and regenerates `docs/conformance.md` (never edit it by hand). A value counts as validated only if the engine computes the parameter and it is within the tolerance of the expected value. The file ends with one floor line per parameter per case (validated and expected rows). The task fails and leaves the file unchanged if a parameter validates fewer values, loses expected rows, gains unvalidated rows or disappears, if a floor line is malformed, or if a non-empty previous file holds no floor. Floors can only go up. The file is written to a temporary file then renamed.

## Usage

The command-line application runs the same commands as the UI and the MCP server (`caladrius-cli commands` lists them with their titles; `caladrius-cli commands --format json` adds the JSON schema of the parameters and results of each).

```sh
# NCA of the Theoph dataset, one table (subject, parameter, value, reason) on standard output
cargo run -q -p caladrius-cli -- nca.run --csv oracle/data/theoph.csv --param route=extravascular --format csv

# the same as JSON, linear trapezoids, one subject
cargo run -q -p caladrius-cli -- nca.run --csv oracle/data/theoph.csv --param route=extravascular --param options.auc_method=linear --param subject=1

# a model fit; the project file keeps the work between calls
cargo run -q -p caladrius-cli -- fit.run --csv oracle/data/indometh.csv --project indo.caladrius.json --param subject=1 --param model=pk1.iv_bolus --param weighting=inv_y2 --format csv
cargo run -q -p caladrius-cli -- project.describe --project indo.caladrius.json
```

`--param KEY=VALUE` sets one parameter (JSON when the value parses, text otherwise; `a.b=1` nests; `csv=@file` reads a file), `--json FILE` (or `-` for standard input) gives them all as an object, `--csv FILE` imports a worksheet first and is used by commands that take a `worksheet`, `--project FILE` loads and saves the project (worksheets, analyses with their options and results, and the history of the commands), `--format csv` prints the main table of an analysis (`--table NAME` picks another). What an import guessed (roles, units) and unit warnings go to standard error. A failure prints `error: <code>: <message>` and exits with code 1.

A saved project contains the data it was built from, twice: in its worksheets, and in the history, which keeps the parameters of each command up to 64 KiB (an import keeps its CSV text; from the second invocation on, a `project.load` entry can hold the previous project text while that is under 64 KiB). This is provenance, and it makes the file larger and de-identification harder: before sharing a project, remove the `history` array from the JSON file (the project loads without it; `history_restored` is then false), or start from `project.new` and import only what you want to keep.

## MCP

`caladrius-mcp` is a Model Context Protocol server on standard input and output (JSON-RPC 2.0, one message per line; no network). It exposes every command of the registry as a tool: `data_import`, `nca_run`, `fit_run`, `model_simulate`, `export_table`... (the command id with `.` written `_`; the original id is accepted too). Each tool's `inputSchema` and `outputSchema` are the JSON schemas of the command, a result is JSON text content plus `structuredContent`, and a failing command is a tool error (`isError: true`) whose text is `<code>: <message>`. The server keeps one project for the session: worksheets and analyses persist between calls, `project_save` returns the whole project and `project_load` restores it.

Limits: the server handles one request at a time, so a long fit blocks the loop until it ends (no progress notifications, and a `ping` sent meanwhile is answered afterwards); a line longer than 64 MiB is answered with an error and skipped, a batch is limited to 100 requests, a message without an `id` is a notification and is never answered, and a byte order mark on the first line is ignored.

Build the binary once, then register it in the agent host.

```sh
cargo build --release -p caladrius-mcp      # target/release/caladrius-mcp
```

Claude Code, in `.mcp.json` at the root of a project (or `claude mcp add caladrius -- /path/to/caladrius-mcp`):

```json
{
  "mcpServers": {
    "caladrius": {
      "command": "/path/to/caladrius/target/release/caladrius-mcp",
      "args": []
    }
  }
}
```

Any other host that starts MCP servers over stdio (Pi Durable, dsh, an Apothicaire agent): run `/path/to/caladrius-mcp` with no arguments as a child process and speak the protocol on its standard input and output; a quick check by hand:

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"hand","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | caladrius-mcp
```
