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

use std::path::Path;
use std::process::ExitCode;

use caladrius_ui::{Action, Request, UiApp};

const USAGE: &str = "\
Caladrius: pharmacokinetic analysis.

usage:
  caladrius [FILE.csv]     open the application, optionally with a CSV file to import
  caladrius --help         show this message
  caladrius --version      show the version

Open a CSV with the button or drop it on the window; the table is shown as it will be read
before anything is imported. The same commands are available without a window in
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
}

impl Desktop {
    fn open(&mut self, path: &Path) {
        match read(path) {
            Ok((name, bytes)) => self.ui.load_csv(&name, &bytes),
            Err(message) => self.ui.perform(vec![Action::Notice(message)]),
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
        self.ui.ui(ctx);
        for request in self.ui.take_requests() {
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
            }
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
            let mut desktop = Desktop { ui: UiApp::new() };
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
        let mut desktop = Desktop { ui: UiApp::new() };
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
