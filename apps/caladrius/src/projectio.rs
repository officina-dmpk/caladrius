//! Reading and writing project files: the part of saving and opening that touches the disk. The
//! interface hands over the bytes of the document and gets back a file name or a sentence.

use std::path::{Path, PathBuf};

/// The name to show for a path: its file name.
pub fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Writes `bytes` to `path` without leaving a half-written file behind: the bytes go to a
/// neighbouring temporary file which then replaces the target. The error says which file and why.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temp: PathBuf = path.to_path_buf();
    let mut name = temp
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    temp.set_file_name(name);
    std::fs::write(&temp, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("cannot replace {}: {e}", path.display())
    })
}

/// Reads a project file; the error says which file and why.
pub fn read(path: &Path) -> Result<(String, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok((display_name(path), bytes))
}

/// The path with the project extension when the person typed a bare name.
pub fn with_extension(path: PathBuf) -> PathBuf {
    let name = display_name(&path).to_ascii_lowercase();
    if name.ends_with(".json") {
        path
    } else {
        let mut text = path.into_os_string();
        text.push(".caladrius.json");
        PathBuf::from(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("caladrius-io-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_project_is_written_whole_and_read_back() {
        let dir = scratch("rw");
        let path = dir.join("study.caladrius.json");
        write(&path, b"first").unwrap();
        write(&path, b"second").unwrap();
        let (name, bytes) = read(&path).unwrap();
        assert_eq!(
            (name.as_str(), bytes.as_slice()),
            ("study.caladrius.json", b"second".as_slice())
        );
        // No temporary file is left next to it.
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["study.caladrius.json".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_written_or_read_says_which_one() {
        let e = write(Path::new("/no/such/dir/p.caladrius.json"), b"x").unwrap_err();
        assert!(
            e.contains("cannot write") && e.contains("p.caladrius.json"),
            "{e}"
        );
        let e = read(Path::new("/no/such/dir/p.caladrius.json")).unwrap_err();
        assert!(e.contains("cannot read"), "{e}");
    }

    #[test]
    fn a_bare_name_gets_the_project_extension() {
        assert_eq!(
            with_extension(PathBuf::from("dir/study")),
            PathBuf::from("dir/study.caladrius.json")
        );
        assert_eq!(
            with_extension(PathBuf::from("dir/study.caladrius.json")),
            PathBuf::from("dir/study.caladrius.json")
        );
        assert_eq!(
            with_extension(PathBuf::from("a.JSON")),
            PathBuf::from("a.JSON")
        );
    }
}
