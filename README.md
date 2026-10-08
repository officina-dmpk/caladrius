# Caladrius

Open-source pharmacokinetic analysis in Rust: non-compartmental analysis, individual compartmental models, weighted least-squares fitting and plots, with a native desktop UI (egui) and a WebAssembly demo. Every analysis is a command with a stable id, so the same calculations are available from the UI, the command line and an MCP server for agents. Caladrius is the calculation layer of Apothicaire, a local DMPK assistant.

Status: step 0 (skeleton). See `AGENTS.md` for the contract and `board/INDEX.md` for progress.

Named after the caladrius, the white bird of Roman legend said to take a sick person's illness away as it flies off.

## Trademark notice

Caladrius is an independent project and is not affiliated with, endorsed by or derived from Certara. Phoenix and WinNonlin are trademarks of Certara. Caladrius aims at compatible conventions for standard pharmacokinetic calculations, reimplemented from public textbooks and documentation.

## License

MIT OR Apache-2.0. Third-party data and assets are listed in `ATTRIBUTION.md`.
