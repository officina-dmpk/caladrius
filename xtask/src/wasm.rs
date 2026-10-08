//! `cargo xtask wasm`: layers L0 to L3 must compile for WebAssembly (AGENTS.md golden rule 9).
//!
//! The task installs nothing: if the target is missing it says which `rustup` command to run.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use crate::console;
use crate::error::{Result, XtaskError};
use crate::layers::{LAYER_TABLE, Layer};

/// The compilation target of the browser demo.
const TARGET: &str = "wasm32-unknown-unknown";

/// The crates that must compile for wasm: layers L0 to L3, in table order.
fn wasm_crates<'a>(table: &[(&'a str, Layer)]) -> Vec<&'a str> {
    table
        .iter()
        .filter(|(_, layer)| layer.compiles_to_wasm())
        .map(|(name, _)| *name)
        .collect()
}

/// Arguments of the `cargo` invocation that checks `crates` for the wasm target.
fn check_args(crates: &[&str]) -> Vec<String> {
    let mut args = vec!["check".to_owned(), "--target".to_owned(), TARGET.to_owned()];
    for name in crates {
        args.push("-p".to_owned());
        args.push((*name).to_owned());
    }
    args
}

/// The message shown when the target is missing: the exact command that fixes it.
fn missing_target_message() -> String {
    format!(
        "the {TARGET} target is not installed for the active toolchain; install it with:\n    rustup target add {TARGET}"
    )
}

/// Whether the active toolchain ships the standard library for the wasm target.
///
/// `rustc --print target-libdir` prints where that library lives; the directory exists only once the target is installed.
fn target_installed() -> Result<bool> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(&rustc)
        .args(["--print", "target-libdir", "--target", TARGET])
        .output()
        .map_err(|source| {
            XtaskError::new(format!("cannot run {}: {source}", rustc.to_string_lossy()))
        })?;
    if !output.status.success() {
        return Err(XtaskError::new(format!(
            "`rustc --print target-libdir --target {TARGET}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let libdir = String::from_utf8_lossy(&output.stdout);
    Ok(PathBuf::from(libdir.trim()).is_dir())
}

/// Runs `cargo check` for the wasm target on layers L0 to L3.
pub fn run() -> Result<()> {
    if !target_installed()? {
        return Err(XtaskError::new(missing_target_message()));
    }
    let crates = wasm_crates(LAYER_TABLE);
    let args = check_args(&crates);
    console::out(&format!("wasm: cargo {}", args.join(" ")));
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(&cargo)
        .args(&args)
        .status()
        .map_err(|source| {
            XtaskError::new(format!("cannot run {}: {source}", cargo.to_string_lossy()))
        })?;
    if status.success() {
        console::out(&format!(
            "wasm: ok ({} crates compile for {TARGET})",
            crates.len()
        ));
        Ok(())
    } else {
        Err(XtaskError::new(format!(
            "wasm: cargo check failed for {TARGET} ({status})"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_l0_to_l3_are_checked_in_table_order() {
        assert_eq!(
            wasm_crates(LAYER_TABLE),
            [
                "caladrius-nca",
                "caladrius-models",
                "caladrius-fit",
                "caladrius-project",
                "caladrius-engine",
                "caladrius-ui",
            ]
        );
    }

    #[test]
    fn test_apps_and_tool_layers_are_not_checked() {
        let table = [
            ("a", Layer::L0),
            ("b", Layer::Test),
            ("c", Layer::App),
            ("d", Layer::Tool),
            ("e", Layer::L3),
        ];
        assert_eq!(wasm_crates(&table), ["a", "e"]);
    }

    #[test]
    fn cargo_check_arguments_name_the_target_and_each_crate() {
        assert_eq!(
            check_args(&["caladrius-nca", "caladrius-ui"]),
            [
                "check",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "caladrius-nca",
                "-p",
                "caladrius-ui"
            ]
        );
    }

    #[test]
    fn the_missing_target_message_gives_the_rustup_command() {
        assert!(missing_target_message().ends_with("rustup target add wasm32-unknown-unknown"));
    }
}
