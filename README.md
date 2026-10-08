# Caladrius

Open-source pharmacokinetic analysis in Rust: non-compartmental analysis, individual compartmental models, weighted least-squares fitting and plots, with a native desktop UI (egui) and a WebAssembly demo. Every analysis is a command with a stable id, so the same calculations are available from the UI, the command line and an MCP server for agents. Caladrius is the calculation layer of Apothicaire, a local DMPK assistant.

Status: step 0 (skeleton). See `AGENTS.md` for the contract and `board/INDEX.md` for progress.

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

`cargo xtask conformance` runs `caladrius-nca` on every case of `oracle/expected/` and regenerates `docs/conformance.md` (never edit it by hand). A value counts as validated only if the engine computes the parameter and it is within the tolerance of the expected value. The file ends with one floor line per parameter per case (validated and expected rows). The task fails and leaves the file unchanged if a parameter validates fewer values, loses expected rows, gains unvalidated rows or disappears, if a floor line is malformed, or if a non-empty previous file holds no floor. Floors can only go up. The file is written to a temporary file then renamed.
