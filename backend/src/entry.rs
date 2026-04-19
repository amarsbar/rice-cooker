const CANDIDATES: &[&str] = &[
    "shell.qml",
    "ii/shell.qml",
    "quickshell/shell.qml",
    ".config/quickshell/shell.qml",
];

pub fn find_shell_qml(rice_root: &std::path::Path) -> anyhow::Result<Option<std::path::PathBuf>> {
    for &candidate in CANDIDATES {
        let path = rice_root.join(candidate);
        if path.is_file() {
            return Ok(Some(std::path::PathBuf::from(candidate)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_file(dir: &std::path::Path, rel: &str) {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, b"").unwrap();
    }

    #[test]
    fn empty_dir_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn root_shell_qml_found() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), "shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(result, Some(std::path::PathBuf::from("shell.qml")));
    }

    #[test]
    fn ii_shell_qml_found_when_root_missing() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), "ii/shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(result, Some(std::path::PathBuf::from("ii/shell.qml")));
    }

    #[test]
    fn quickshell_shell_qml_found_when_above_missing() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), "quickshell/shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(
            result,
            Some(std::path::PathBuf::from("quickshell/shell.qml"))
        );
    }

    #[test]
    fn config_quickshell_shell_qml_found_when_others_missing() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), ".config/quickshell/shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(
            result,
            Some(std::path::PathBuf::from(".config/quickshell/shell.qml"))
        );
    }

    #[test]
    fn root_wins_over_ii_when_both_present() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), "shell.qml");
        make_file(tmp.path(), "ii/shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap();
        assert_eq!(result, Some(std::path::PathBuf::from("shell.qml")));
    }

    #[test]
    fn returned_path_is_relative() {
        let tmp = tempfile::tempdir().unwrap();
        make_file(tmp.path(), "ii/shell.qml");
        let result = find_shell_qml(tmp.path()).unwrap().unwrap();
        assert!(
            result.is_relative(),
            "path must be relative, got: {result:?}"
        );
        assert_eq!(result, std::path::PathBuf::from("ii/shell.qml"));
    }
}
