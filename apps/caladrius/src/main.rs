#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! App layer: the desktop application, a window around `caladrius-ui`. This is where files are
//! read: the file dialog, a path on the command line and files dropped on the window all end in
//! `UiApp::load_csv(name, bytes)`; the interface itself never touches the disk.

mod config;
mod projectio;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use caladrius_ui::projectfile::is_project_file;
use caladrius_ui::{Action, Request, UiApp};

const USAGE: &str = "\
Caladrius: pharmacokinetic analysis.

usage:
  caladrius [FILE]         open the application, optionally with a CSV file to import or a
                           saved project (FILE.caladrius.json)
  caladrius --help         show this message
  caladrius --version      show the version

Open a CSV with the button or drop it on the window; the table is shown as it will be read
before anything is imported. Projects are saved and opened from the File menu (Ctrl+S, Ctrl+O). The same commands are available without a window in
`caladrius-cli` and to agents in `caladrius-mcp`.";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Launch {
    Help,
    Version,
    Window(Option<String>),
}

fn parse(args: &[String]) -> Result<Launch, String> {
    match args {
        [] => Ok(Launch::Window(None)),
        [a] if a == "--help" || a == "-h" => Ok(Launch::Help),
        [a] if a == "--version" || a == "-V" => Ok(Launch::Version),
        [a] if a.starts_with('-') => Err(format!("unknown option `{a}`; see `caladrius --help`")),
        [path] => Ok(Launch::Window(Some(path.clone()))),
        [_, extra, ..] => Err(format!(
            "unexpected argument `{extra}`: open one file at a time; see `caladrius --help`"
        )),
    }
}

/// Reads a file for the interface; the error says which file and why.
fn read(path: &Path) -> Result<(String, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Ok((name, bytes))
}

struct Desktop {
    ui: UiApp,
    /// The file the project is saved in (the interface only knows its name).
    project_path: Option<PathBuf>,
    /// The title last given to the window.
    title: String,
}

impl Desktop {
    fn new(ui: UiApp) -> Desktop {
        Desktop {
            ui,
            project_path: None,
            title: String::new(),
        }
    }

    /// A file given on the command line or dropped on the window: a project or a CSV.
    fn open(&mut self, path: &Path) {
        let name = projectio::display_name(path);
        if is_project_file(&name) {
            match projectio::read(path) {
                Ok((name, bytes)) => {
                    self.ui.open_project_guarded(&name, bytes);
                    self.follow_project(path);
                }
                Err(message) => self.ui.perform(vec![Action::Notice(message)]),
            }
            return;
        }
        match read(path) {
            Ok((name, bytes)) => self.ui.load_csv(&name, &bytes),
            Err(message) => self.ui.perform(vec![Action::Notice(message)]),
        }
    }

    /// Reads the stored settings and the system locale into the interface. With no stored
    /// settings (a first start) the locale suggests the decimal mark, and the settings page
    /// shows that it did.
    fn start_settings(&mut self) {
        self.ui
            .set_system_locale(config::system_locale().as_deref());
        match config::read_settings() {
            Ok(Some(text)) => {
                self.ui.set_settings_json(&text);
            }
            Ok(None) => self.ui.apply_locale_suggestion(),
            Err(message) => self.ui.perform(vec![Action::Notice(message)]),
        }
    }

    /// Remembers `path` as the project's file when the interface now holds that file.
    fn follow_project(&mut self, path: &Path) {
        if self.ui.file_name() == Some(projectio::display_name(path).as_str()) {
            self.project_path = Some(path.to_path_buf());
        }
    }

    fn pick_project(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("Caladrius project", &["json"])
            .set_title("Open a project")
            .pick_file();
        if let Some(path) = picked {
            match projectio::read(&path) {
                Ok((name, bytes)) => {
                    if self.ui.open_project(&name, &bytes) {
                        self.project_path = Some(path);
                    }
                }
                Err(message) => self.ui.perform(vec![Action::Notice(message)]),
            }
        }
    }

    fn save_project(&mut self, suggested: &str, bytes: &[u8], ask: bool) {
        let target = match (&self.project_path, ask) {
            (Some(path), false) => Some(path.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("Caladrius project", &["json"])
                .set_title("Save the project")
                .set_file_name(suggested)
                .save_file()
                .map(projectio::with_extension),
        };
        let Some(path) = target else {
            self.ui.project_save_cancelled();
            return;
        };
        match projectio::write(&path, bytes) {
            Ok(()) => {
                let name = projectio::display_name(&path);
                self.project_path = Some(path);
                self.ui.project_saved(&name);
            }
            Err(message) => self.ui.project_save_failed(&message),
        }
    }

    /// Does what the interface asked, until it has nothing more to ask.
    fn serve(&mut self, ctx: &egui::Context) {
        loop {
            let requests = self.ui.take_requests();
            if requests.is_empty() {
                break;
            }
            for request in requests {
                match request {
                    Request::PickCsv => {
                        let picked = rfd::FileDialog::new()
                            .add_filter("CSV or text", &["csv", "txt", "tsv"])
                            .set_title("Open a CSV file")
                            .pick_file();
                        if let Some(path) = picked {
                            self.open(&path);
                        }
                    }
                    Request::OpenProject => self.pick_project(),
                    Request::SaveProject {
                        suggested,
                        bytes,
                        ask,
                    } => self.save_project(&suggested, &bytes, ask),
                    Request::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Request::SaveSettings(text) => {
                        if let Err(message) = config::write_settings(&text) {
                            self.ui.perform(vec![Action::Notice(message)]);
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Files dropped on the window arrive as paths on a desktop.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open(&path);
            }
        }
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.ui.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.ui.ui(ctx);
        self.serve(ctx);
        if self.ui.file_name().is_none() {
            self.project_path = None;
        }
        let title = self.ui.window_title();
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for (i, a) in std::env::args_os().skip(1).enumerate() {
        match a.into_string() {
            Ok(text) => args.push(text),
            Err(_) => {
                eprintln!("error: argument {} is not valid text", i + 1);
                return ExitCode::from(1);
            }
        }
    }
    let launch = match parse(&args) {
        Ok(l) => l,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };
    let first_file = match launch {
        Launch::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Launch::Version => {
            println!("caladrius {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Launch::Window(file) => file,
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Caladrius")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_drag_and_drop(true),
        ..eframe::NativeOptions::default()
    };
    let result = eframe::run_native(
        "Caladrius",
        options,
        Box::new(move |_cc| {
            let mut desktop = Desktop::new(UiApp::new());
            desktop.start_settings();
            if let Some(path) = &first_file {
                desktop.open(Path::new(path));
            }
            Ok(Box::new(desktop))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: the window could not be opened: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_command_line_is_read() {
        assert_eq!(parse(&args(&[])).unwrap(), Launch::Window(None));
        assert_eq!(parse(&args(&["--help"])).unwrap(), Launch::Help);
        assert_eq!(parse(&args(&["-h"])).unwrap(), Launch::Help);
        assert_eq!(parse(&args(&["--version"])).unwrap(), Launch::Version);
        assert_eq!(
            parse(&args(&["data.csv"])).unwrap(),
            Launch::Window(Some("data.csv".to_owned()))
        );
        assert!(
            parse(&args(&["--bogus"]))
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(
            parse(&args(&["a.csv", "b.csv"]))
                .unwrap_err()
                .contains("one file")
        );
    }

    #[test]
    fn an_unreadable_file_says_which_one() {
        let e = read(Path::new("/no/such/dir/x.csv")).unwrap_err();
        assert!(e.contains("cannot read") && e.contains("x.csv"), "{e}");
    }

    #[test]
    fn a_file_is_read_with_its_name() {
        let dir = std::env::temp_dir().join(format!("caladrius-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("study.csv");
        std::fs::write(&path, "time,conc\n0,1\n").unwrap();
        let (name, bytes) = read(&path).unwrap();
        assert_eq!(
            (name.as_str(), bytes.as_slice()),
            ("study.csv", b"time,conc\n0,1\n".as_slice())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_interface_gets_the_bytes_and_shows_the_preview() {
        let mut desktop = Desktop::new(UiApp::new());
        let dir = std::env::temp_dir().join(format!("caladrius-app-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.csv");
        std::fs::write(&path, "time,conc,dose\n0,0,10\n1,5,10\n2,3,10\n").unwrap();
        desktop.open(&path);
        assert!(desktop.ui.pending_import().is_some());
        desktop.open(&dir.join("missing.csv"));
        assert!(desktop.ui.notice_text().unwrap().contains("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
