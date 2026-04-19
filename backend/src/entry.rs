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
    fn each_candidate_is_found_in_isolation() {
        for candidate in [
            "shell.qml",
            "ii/shell.qml",
            "quickshell/shell.qml",
            ".config/quickshell/shell.qml",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            make_file(tmp.path(), candidate);
            let result = find_shell_qml(tmp.path()).unwrap();
            assert_eq!(
                result.as_deref(),
                Some(std::path::Path::new(candidate)),
                "failed for candidate {candidate:?}"
            );
        }
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
