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
    /// The project file that was dropped or given on the command line and is waiting for the
    /// answer to "save the changes?": its token and path. The path becomes the project's file
    /// only when the interface says that file was opened.
    queued: Option<(u64, PathBuf)>,
    next_token: u64,
}

impl Desktop {
    fn new(ui: UiApp) -> Desktop {
        Desktop {
            ui,
            project_path: None,
            title: String::new(),
            queued: None,
            next_token: 1,
        }
    }

    /// A file given on the command line or dropped on the window: a project or a CSV.
    fn open(&mut self, path: &Path) {
        let name = projectio::display_name(path);
        if is_project_file(&name) {
            match projectio::read(path) {
                Ok((name, bytes)) => {
                    let token = self.next_token;
                    self.next_token += 1;
                    self.queued = Some((token, path.to_path_buf()));
                    self.ui.open_project_guarded(&name, bytes, Some(token));
                    self.sync();
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
        self.start_settings_from(config::system_locale(), config::read_settings());
    }

    fn start_settings_from(
        &mut self,
        locale: Option<String>,
        stored: Result<Option<String>, String>,
    ) {
        self.ui.set_system_locale(locale.as_deref());
        match stored {
            Ok(Some(text)) => {
                self.ui.set_settings_json(&text);
            }
            Ok(None) => self.ui.apply_locale_suggestion(),
            Err(message) => self.ui.perform(vec![Action::Notice(message)]),
        }
    }

    /// Takes what the interface reports about files: a dropped project that was opened (after
    /// the question, if there was one) becomes the project's file; a new project has none.
    fn sync(&mut self) {
        if let Some(token) = self.ui.take_opened_token() {
            if let Some((queued, path)) = self.queued.take() {
                if queued == token {
                    self.project_path = Some(path);
                } else {
                    self.queued = Some((queued, path));
                }
            }
        }
        if self.ui.file_name().is_none() {
            self.project_path = None;
        }
    }

    fn pick_project(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("Caladrius project", &["json"])
            .set_title("Open a project")
            .pick_file();
        if let Some(path) = picked {
            self.open_project_file(path);
        }
    }

    /// Opens the project file the person chose in the dialog (the question about unsaved
    /// changes was asked before the dialog). It becomes the project's file only if it opened.
    fn open_project_file(&mut self, path: PathBuf) {
        match projectio::read(&path) {
            Ok((name, bytes)) => {
                if self.ui.open_project(&name, &bytes) {
                    self.project_path = Some(path);
                }
            }
            Err(message) => self.ui.perform(vec![Action::Notice(message)]),
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
        self.sync();
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

    const CSV_ONE: &str = "time,conc,dose\n0,0,10\n1,5,10\n2,3,10\n";
    const CSV_TWO: &str = "time,conc,dose\n0,0,20\n1,9,20\n2,4,20\n4,2,20\n";

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("caladrius-desk-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The bytes of a saved project holding one worksheet made from `csv`.
    fn project_with(csv: &str) -> Vec<u8> {
        let mut ui = UiApp::new();
        ui.load_csv("study.csv", csv.as_bytes());
        ui.perform(vec![Action::ImportConfirm, Action::SaveProject]);
        ui.take_requests()
            .into_iter()
            .find_map(|r| match r {
                Request::SaveProject { bytes, .. } => Some(bytes),
                _ => None,
            })
            .unwrap()
    }

    fn make_dirty(desktop: &mut Desktop) {
        desktop.ui.load_csv("more.csv", CSV_ONE.as_bytes());
        desktop.ui.perform(vec![Action::ImportConfirm]);
        assert!(desktop.ui.is_dirty());
    }

    #[test]
    fn a_dropped_project_waits_for_the_answer_and_save_never_overwrites_the_wrong_file() {
        let ctx = egui::Context::default();
        // The dropped file has another name, and the same name as the current one.
        for same_name in [false, true] {
            let dir = scratch(if same_name { "same" } else { "other" });
            let first = dir.join("a").join("p.caladrius.json");
            let second = dir.join("b").join(if same_name {
                "p.caladrius.json"
            } else {
                "q.caladrius.json"
            });
            for path in [&first, &second] {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            }
            std::fs::write(&first, project_with(CSV_ONE)).unwrap();
            std::fs::write(&second, project_with(CSV_TWO)).unwrap();
            let first_bytes = std::fs::read(&first).unwrap();
            let second_bytes = std::fs::read(&second).unwrap();

            let mut desktop = Desktop::new(UiApp::new());
            desktop.open(&first);
            assert_eq!(desktop.project_path.as_deref(), Some(first.as_path()));
            make_dirty(&mut desktop);
            // The second file only waits behind the question: the project's file is still the first.
            desktop.open(&second);
            assert!(desktop.ui.guard().is_some());
            assert_eq!(desktop.project_path.as_deref(), Some(first.as_path()));
            assert_eq!(desktop.ui.file_name(), Some("p.caladrius.json"));
            // Cancel keeps everything as it was.
            desktop.ui.perform(vec![Action::GuardCancel]);
            desktop.sync();
            assert_eq!(desktop.project_path.as_deref(), Some(first.as_path()));
            // Ask again, answer the question with "Don't save": the second file is now the project's file.
            desktop.open(&second);
            desktop.ui.perform(vec![Action::GuardDiscard]);
            desktop.sync();
            assert_eq!(desktop.project_path.as_deref(), Some(second.as_path()));
            // Edit and save: the second file is written, the first is untouched.
            make_dirty(&mut desktop);
            desktop.ui.perform(vec![Action::SaveProject]);
            desktop.serve(&ctx);
            assert!(!desktop.ui.is_dirty());
            assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
            assert_ne!(std::fs::read(&second).unwrap(), second_bytes);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn answering_save_writes_the_current_file_first_then_opens_the_dropped_one() {
        let ctx = egui::Context::default();
        let dir = scratch("save-first");
        let first = dir.join("a.caladrius.json");
        let second = dir.join("b.caladrius.json");
        std::fs::write(&first, project_with(CSV_ONE)).unwrap();
        std::fs::write(&second, project_with(CSV_TWO)).unwrap();
        let first_bytes = std::fs::read(&first).unwrap();
        let second_bytes = std::fs::read(&second).unwrap();
        let mut desktop = Desktop::new(UiApp::new());
        desktop.open(&first);
        make_dirty(&mut desktop);
        desktop.open(&second);
        desktop.ui.perform(vec![Action::GuardSave]);
        desktop.serve(&ctx);
        desktop.sync();
        // The changes went to the first file, then the second was opened and is now the file.
        assert_ne!(std::fs::read(&first).unwrap(), first_bytes);
        assert_eq!(std::fs::read(&second).unwrap(), second_bytes);
        assert_eq!(desktop.project_path.as_deref(), Some(second.as_path()));
        assert_eq!(desktop.ui.file_name(), Some("b.caladrius.json"));
        assert!(!desktop.ui.is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_chosen_in_the_dialog_is_the_file_only_if_it_opened_and_new_forgets_it() {
        let dir = scratch("dialog");
        let good = dir.join("good.caladrius.json");
        let bad = dir.join("bad.json");
        std::fs::write(&good, project_with(CSV_ONE)).unwrap();
        std::fs::write(&bad, "{ not a project").unwrap();
        let mut desktop = Desktop::new(UiApp::new());
        desktop.open_project_file(good.clone());
        assert_eq!(desktop.project_path.as_deref(), Some(good.as_path()));
        // A file that is not a project, and one that is not there: nothing changes, a sentence says why.
        desktop.open_project_file(bad);
        assert_eq!(desktop.project_path.as_deref(), Some(good.as_path()));
        assert!(
            desktop
                .ui
                .notice_text()
                .unwrap()
                .contains("cannot be opened")
        );
        desktop.open_project_file(dir.join("missing.caladrius.json"));
        assert_eq!(desktop.project_path.as_deref(), Some(good.as_path()));
        assert!(desktop.ui.notice_text().unwrap().contains("cannot read"));
        // A new project has no file.
        desktop.ui.perform(vec![Action::NewProject]);
        desktop.sync();
        assert!(desktop.project_path.is_none());
        // A dropped file that is not named like a project is read as a CSV, not as a project.
        let csv = dir.join("study.csv");
        std::fs::write(&csv, CSV_ONE).unwrap();
        desktop.open(&csv);
        assert!(desktop.ui.pending_import().is_some());
        assert!(desktop.project_path.is_none());
        let json = dir.join("data.json");
        std::fs::write(&json, "{}").unwrap();
        desktop.open(&json);
        assert!(desktop.project_path.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_settings_at_start_come_from_the_file_or_from_the_locale_or_say_why_not() {
        use caladrius_ui::settings::{DecimalMark, Settings};
        // No file: a first start, the locale suggests the comma and the page will show it.
        let mut desktop = Desktop::new(UiApp::new());
        desktop.start_settings_from(Some("fr-FR".to_owned()), Ok(None));
        assert_eq!(desktop.ui.settings.decimal_mark, DecimalMark::Comma);
        assert!(desktop.ui.settings.locale.mark_from_system);
        // A stored file wins over the locale.
        let stored = Settings {
            significant_digits: 6,
            ..Settings::default()
        };
        let mut desktop = Desktop::new(UiApp::new());
        desktop.start_settings_from(Some("fr-FR".to_owned()), Ok(Some(stored.to_json())));
        assert_eq!(desktop.ui.settings.significant_digits, 6);
        assert_eq!(desktop.ui.settings.decimal_mark, DecimalMark::Point);
        assert_eq!(desktop.ui.settings.locale.system.as_deref(), Some("fr-FR"));
        // A file that cannot be read: the defaults and a sentence; so for one that is damaged.
        let mut desktop = Desktop::new(UiApp::new());
        desktop.start_settings_from(
            None,
            Err("cannot read settings.json: access denied".to_owned()),
        );
        assert_eq!(desktop.ui.settings, Settings::default());
        assert_eq!(
            desktop.ui.notice_text(),
            Some("cannot read settings.json: access denied")
        );
        let mut desktop = Desktop::new(UiApp::new());
        desktop.start_settings_from(None, Ok(Some("{ broken".to_owned())));
        assert_eq!(desktop.ui.settings, Settings::default());
        assert!(
            desktop
                .ui
                .notice_text()
                .unwrap()
                .ends_with("the default settings are used.")
        );
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
