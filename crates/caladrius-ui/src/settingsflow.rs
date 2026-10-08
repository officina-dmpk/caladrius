//! How the settings reach the application: a change is applied at once (number display, theme),
//! remembered for the program to store ([`Request::SaveSettings`]), and used by the next new
//! analysis. The program hands stored settings and the system locale in; it never gets a setting
//! it did not store.

use serde_json::Value;

use crate::app::{Notice, NoticeKind, Request, Selection, UiApp};
use crate::fmt::{self, Display};
use crate::settings::{DecimalMark, Settings, ThemeChoice};
use crate::theme::ThemeMode;

impl UiApp {
    /// The number display the settings ask for.
    fn display(&self) -> Display {
        Display {
            digits: self.settings.significant_digits,
            comma: self.settings.decimal_mark == DecimalMark::Comma,
        }
    }

    /// Makes the settings take effect on screen: the number display and the theme.
    pub(crate) fn apply_settings(&mut self) {
        fmt::set_display(self.display());
        self.state.mode = self.settings.theme.mode(self.system_theme);
    }

    /// Called every frame: the number display is a property of the thread that draws, and the
    /// system theme can change while the window is open.
    pub(crate) fn follow_settings(&mut self, system: Option<ThemeMode>) {
        fmt::set_display(self.display());
        if system != self.system_theme {
            self.system_theme = system;
            if self.settings.theme == ThemeChoice::System {
                self.state.mode = self.settings.theme.mode(system);
            }
        }
    }

    /// Asks the program to store the settings when they differ from what it stored last.
    pub(crate) fn remember_settings(&mut self) {
        let text = self.settings.to_json();
        if text != self.saved_settings {
            self.saved_settings.clone_from(&text);
            self.requests.push(Request::SaveSettings(text));
        }
    }

    fn settings_notice(&mut self, text: String, kind: NoticeKind) {
        self.notice = Some(Notice { kind, text });
    }

    /// The settings page.
    pub(crate) fn open_settings(&mut self) {
        self.state.selection = Selection::Settings;
    }

    /// Changes one setting (from the page, or the palette): refused values say why.
    pub(crate) fn set_setting(&mut self, key: &str, value: Value) {
        match self.settings.set(key, value) {
            Ok(()) => {
                self.apply_settings();
                self.remember_settings();
            }
            Err(message) => self.settings_notice(message, NoticeKind::Error),
        }
    }

    /// Puts one setting back to its default.
    pub(crate) fn reset_setting(&mut self, key: &str) {
        match Settings::default_of(key) {
            Some(value) => self.set_setting(key, value),
            None => self.settings_notice(format!("there is no setting {key}"), NoticeKind::Error),
        }
    }

    /// Takes the decimal mark the system locale suggests (the person asked for it).
    pub(crate) fn use_system_suggestion(&mut self) {
        if self.settings.suggest_from_system() {
            self.apply_settings();
            self.remember_settings();
        } else {
            self.settings_notice(
                "The system reported no locale that suggests a decimal mark.".to_owned(),
                NoticeKind::Info,
            );
        }
    }

    /// Stored settings, read by the program at start. Text that cannot be read leaves the
    /// defaults in place and says so.
    pub fn set_settings_json(&mut self, text: &str) -> bool {
        match Settings::from_json(text) {
            Ok(mut stored) => {
                // The locale is read from the system at every start, not from the file.
                stored
                    .locale
                    .system
                    .clone_from(&self.settings.locale.system);
                if stored.locale.system.is_none() {
                    stored.locale.mark_from_system = false;
                }
                self.settings = stored;
                self.saved_settings = self.settings.to_json();
                self.apply_settings();
                true
            }
            Err(message) => {
                self.settings_notice(
                    format!("{message}; the default settings are used."),
                    NoticeKind::Error,
                );
                false
            }
        }
    }

    /// The locale the operating system reports (`fr-FR`), or `None`. It is shown on the settings
    /// page and applied to nothing.
    pub fn set_system_locale(&mut self, tag: Option<&str>) {
        self.settings.note_system_locale(tag);
    }

    /// First start (no stored settings): the system locale suggests the decimal mark. The
    /// suggestion is shown on the settings page as such.
    pub fn apply_locale_suggestion(&mut self) {
        if self.settings.suggest_from_system() {
            self.apply_settings();
            self.remember_settings();
        }
    }

    /// The settings as JSON text, for the program to store.
    pub fn settings_json(&self) -> String {
        self.settings.to_json()
    }
}
