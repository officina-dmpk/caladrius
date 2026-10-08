//! The application's configuration on this machine: where the settings are stored, and the
//! locale the operating system reports. This is platform code (paths, environment, processes);
//! the interface gets the settings text and the locale tag and nothing else.

use std::path::PathBuf;
use std::process::Command;

use crate::projectio;

/// The operating systems whose conventions are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Other,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Other
        }
    }
}

/// The folder for the configuration, from the environment: `%APPDATA%\Caladrius` on Windows,
/// `~/Library/Application Support/Caladrius` on a Mac, `$XDG_CONFIG_HOME/caladrius` or
/// `~/.config/caladrius` elsewhere. `None` when the environment gives no place to start from.
pub fn config_dir(os: Os, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| env(name).filter(|v| !v.trim().is_empty());
    match os {
        Os::Windows => set("APPDATA").map(|d| PathBuf::from(d).join("Caladrius")),
        Os::Mac => set("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library")
                .join("Application Support")
                .join("Caladrius")
        }),
        Os::Other => match set("XDG_CONFIG_HOME") {
            Some(x) => Some(PathBuf::from(x).join("caladrius")),
            None => set("HOME").map(|h| PathBuf::from(h).join(".config").join("caladrius")),
        },
    }
}

/// The file the settings are stored in.
pub fn settings_path() -> Option<PathBuf> {
    config_dir(Os::current(), &|name| std::env::var(name).ok()).map(|d| d.join("settings.json"))
}

/// The stored settings: `Ok(None)` when there is no file yet (a first start).
pub fn read_settings() -> Result<Option<String>, String> {
    let Some(path) = settings_path() else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Stores the settings, creating the folder when needed.
pub fn write_settings(text: &str) -> Result<(), String> {
    let path = settings_path().ok_or_else(|| {
        "the settings cannot be stored: no configuration folder is known".to_owned()
    })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    projectio::write(&path, text.as_bytes())
}

/// The locale from environment variables (`LC_ALL`, then `LC_NUMERIC`, then `LANG`); `C` and
/// `POSIX` name no language.
pub fn locale_from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .filter_map(|name| env(name))
        .map(|v| v.trim().to_owned())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

/// The locale name in the output of `reg query ... /v LocaleName` (`    LocaleName    REG_SZ    fr-FR`).
pub fn locale_from_reg(output: &str) -> Option<String> {
    output
        .lines()
        .find(|l| l.trim_start().starts_with("LocaleName"))
        .and_then(|l| l.split_whitespace().nth(2))
        .map(str::to_owned)
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The locale the operating system reports, or `None` when it cannot be found.
pub fn system_locale() -> Option<String> {
    let env = |name: &str| std::env::var(name).ok();
    match Os::current() {
        Os::Windows => run(
            "reg",
            &[
                "query",
                r"HKCU\Control Panel\International",
                "/v",
                "LocaleName",
            ],
        )
        .and_then(|o| locale_from_reg(&o))
        .or_else(|| locale_from_env(&env)),
        Os::Mac => run("defaults", &["read", "-g", "AppleLocale"])
            .map(|o| o.trim().to_owned())
            .filter(|o| !o.is_empty())
            .or_else(|| locale_from_env(&env)),
        Os::Other => locale_from_env(&env),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn the_configuration_folder_follows_each_systems_convention() {
        assert_eq!(
            config_dir(
                Os::Windows,
                &env(&[("APPDATA", r"C:\Users\x\AppData\Roaming")])
            ),
            Some(PathBuf::from(r"C:\Users\x\AppData\Roaming").join("Caladrius"))
        );
        assert_eq!(
            config_dir(Os::Mac, &env(&[("HOME", "/Users/x")])),
            Some(PathBuf::from(
                "/Users/x/Library/Application Support/Caladrius"
            ))
        );
        assert_eq!(
            config_dir(
                Os::Other,
                &env(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/x")])
            ),
            Some(PathBuf::from("/cfg/caladrius"))
        );
        assert_eq!(
            config_dir(Os::Other, &env(&[("HOME", "/home/x")])),
            Some(PathBuf::from("/home/x/.config/caladrius"))
        );
        // Nothing to start from: no folder, not a guess.
        assert_eq!(config_dir(Os::Windows, &env(&[])), None);
        assert_eq!(config_dir(Os::Other, &env(&[("HOME", "  ")])), None);
    }

    #[test]
    fn the_locale_is_read_from_the_environment_without_guessing() {
        assert_eq!(
            locale_from_env(&env(&[("LANG", "fr_FR.UTF-8")])),
            Some("fr_FR.UTF-8".to_owned())
        );
        // LC_ALL wins; C and POSIX name no language.
        assert_eq!(
            locale_from_env(&env(&[("LC_ALL", "de_DE"), ("LANG", "en_US")])),
            Some("de_DE".to_owned())
        );
        assert_eq!(
            locale_from_env(&env(&[("LC_ALL", "C"), ("LANG", "es_ES")])),
            Some("es_ES".to_owned())
        );
        assert_eq!(locale_from_env(&env(&[("LANG", "POSIX")])), None);
        assert_eq!(locale_from_env(&env(&[])), None);
    }

    #[test]
    fn the_windows_registry_answer_is_read() {
        let output = "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    LocaleName    REG_SZ    fr-FR\r\n\r\n";
        assert_eq!(locale_from_reg(output), Some("fr-FR".to_owned()));
        assert_eq!(
            locale_from_reg("ERROR: The system was unable to find the specified registry key"),
            None
        );
    }

    #[test]
    fn settings_are_written_to_the_configuration_folder_and_read_back() {
        let dir = std::env::temp_dir().join(format!("caladrius-config-{}", std::process::id()));
        let path = dir.join("nested").join("settings.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        projectio::write(&path, b"{\"significant_digits\": 5}").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"significant_digits\": 5}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
