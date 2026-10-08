#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L3: the egui interface. It talks to the engine only through the commands of
//! `caladrius-engine`, holds no numerical code and reads no file: the program around it passes
//! the bytes of a CSV file in ([`UiApp::load_csv`]) and asks for a file picker ([`Request`]).
//!
//! - [`theme`]: the tokens (colours, radii, spacing, font sizes) read from `theme.json`, light and dark.
//! - [`model`]: typed views of the engine's JSON answers.
//! - [`plotdata`]: what the profile plot draws, including the safe semi-log view.
//! - [`app`]: the application, its project tree and its screens ([`import`], [`sheet`], [`nca`]).

pub mod app;
pub mod fit;
pub mod fitform;
pub mod fitplots;
pub mod fitresult;
pub mod flow;
pub mod fmt;
pub mod import;
pub mod model;
pub mod modelinfo;
pub mod modelpick;
pub mod nca;
pub mod plot;
pub mod plotdata;
pub mod projectfile;
pub mod projectmenu;
pub mod sheet;
pub mod sim;
pub mod theme;
pub mod widgets;

pub use app::{Action, Request, Selection, UiApp, UiState};

#[cfg(test)]
mod tests;
pub use theme::{ThemeMode, Tokens};
#[cfg(test)]
mod tests_project;
