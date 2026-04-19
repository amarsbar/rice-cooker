use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use walkdir::WalkDir;

const ALLOWLIST: &[&str] = &[
    "Qt",
    "QtQuick",
    "QtQml",
    "QtCore",
    "QtMultimedia",
    "QtNetwork",
    "QtConcurrent",
    "QtWebEngine",
    "QtWebChannel",
    "QtWayland",
    "QtTest",
    "QtPositioning",
    "QtLocation",
    "QtSvg",
    "QtGraphicalEffects",
    "QtQuick3D",
    "Quickshell",
];

fn import_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*import\s+([A-Z][A-Za-z0-9_.]*)").unwrap())
}

fn block_comment_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // (?s) makes . match newlines so `/* ... */` spanning lines is stripped.
    // Non-greedy `.*?` so consecutive block comments don't fuse into one match.
    RE.get_or_init(|| Regex::new(r"(?s)/\*.*?\*/").unwrap())
}

pub fn detect_missing_plugins(rice_root: &Path) -> Result<Vec<String>> {
    let local = local_first_segments(rice_root)?;
    let re = import_regex();
    let mut missing: BTreeSet<String> = BTreeSet::new();

    for entry in WalkDir::new(rice_root) {
        let entry =
            entry.with_context(|| format!("walking rice tree at {}", rice_root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|e| e.to_str()) != Some("qml") {
            continue;
        }
        let raw = fs::read_to_string(entry.path())
            .with_context(|| format!("reading {}", entry.path().display()))?;
        let stripped = strip_qml_comments(&raw);
        for line in stripped.lines() {
            if let Some(caps) = re.captures(line) {
                let dotted = &caps[1];
                let first = dotted.split('.').next().unwrap_or(dotted);
                if ALLOWLIST.contains(&first) || local.contains(first) {
                    continue;
                }
                missing.insert(first.to_string());
            }
        }
    }

    Ok(missing.into_iter().collect())
}

/// Strip QML/JS-style block and line comments so an `import` inside a comment
/// isn't mistaken for a real plugin dependency. This is best-effort (doesn't
/// understand strings), but it fixes the common multi-line comment case.
fn strip_qml_comments(source: &str) -> String {
    let without_blocks = block_comment_regex().replace_all(source, "");
    without_blocks
        .lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn local_first_segments(rice_root: &Path) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let rd = fs::read_dir(rice_root)
        .with_context(|| format!("reading rice root {}", rice_root.display()))?;
    for entry in rd {
        let entry =
            entry.with_context(|| format!("iterating rice root {}", rice_root.display()))?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(name) = entry.file_name().to_str() {
                out.insert(name.to_string());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn write_qml(dir: &Path, relpath: &str, contents: &str) {
        let p = dir.join(relpath);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    #[test]
    fn allowlisted_imports_are_all_accepted() {
        // Qt*, Qt.labs.*, and Quickshell.* all resolve via first-segment allowlist.
        let dir = tempfile::tempdir().unwrap();
        write_qml(
            dir.path(),
            "shell.qml",
            "import QtQuick 2.15\nimport QtQuick.Controls 2.15\nimport Qt.labs.platform 1.0\n",
        );
        assert!(detect_missing_plugins(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn quickshell_submodule_imports_are_allowed() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(
            dir.path(),
            "shell.qml",
            "import Quickshell\nimport Quickshell.Io\nimport Quickshell.Hyprland\n",
        );
        assert!(detect_missing_plugins(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn unknown_capitalized_import_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(dir.path(), "shell.qml", "import Foo.Bar 1.0\n");
        let missing = detect_missing_plugins(dir.path()).unwrap();
        assert_eq!(missing, vec!["Foo".to_string()]);
    }

    #[test]
    fn local_subdir_import_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Widgets")).unwrap();
        write_qml(dir.path(), "shell.qml", "import Widgets 1.0\n");
        assert!(detect_missing_plugins(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn multiple_missing_plugins_are_deduped_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(
            dir.path(),
            "shell.qml",
            "import Zzz.Sub\nimport Aaa.One\nimport Aaa.Two\n",
        );
        write_qml(dir.path(), "bar/Thing.qml", "import Zzz.Sub 1.0\n");
        let missing = detect_missing_plugins(dir.path()).unwrap();
        assert_eq!(missing, vec!["Aaa".to_string(), "Zzz".to_string()]);
    }

    #[test]
    fn lowercase_string_imports_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(
            dir.path(),
            "shell.qml",
            "import \"components\"\nimport qs.foo\n",
        );
        assert!(detect_missing_plugins(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn leading_whitespace_on_import_is_handled() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(dir.path(), "shell.qml", "    import Foo.Bar 1.0\n");
        assert_eq!(
            detect_missing_plugins(dir.path()).unwrap(),
            vec!["Foo".to_string()]
        );
    }

    #[test]
    fn comment_import_is_not_matched() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(
            dir.path(),
            "shell.qml",
            "// import Foo.Bar 1.0\n/* import Baz.Qux 1.0 */\n",
        );
        assert!(detect_missing_plugins(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn walks_nested_qml_files() {
        let dir = tempfile::tempdir().unwrap();
        write_qml(dir.path(), "shell.qml", "import QtQuick 2.15\n");
        write_qml(dir.path(), "sub/Widget.qml", "import Foo.Bar 1.0\n");
        assert_eq!(
            detect_missing_plugins(dir.path()).unwrap(),
            vec!["Foo".to_string()]
        );
    }
}
