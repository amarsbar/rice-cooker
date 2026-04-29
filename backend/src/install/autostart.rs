use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::paths::Paths;

use super::record::atomic_write_fsync;

const SOURCE_MARKER: &str = "# rice-cooker managed";
const SOURCE_LINE: &str = "source = ~/.config/hypr/rice-cooker.conf";
const SOURCE_LINE_PATH: &str = "~/.config/hypr/rice-cooker.conf";

pub fn install_hypr_autostart(paths: &Paths, name: &str) -> Result<()> {
    ensure_hypr_source(paths)?;
    write_fragment(paths, Some(name))
}

pub fn preflight_hypr_autostart(paths: &Paths) -> Result<()> {
    let _ = source_update(paths)?;
    Ok(())
}

pub fn clear_hypr_autostart(paths: &Paths) -> Result<()> {
    if !paths.hypr_rice_cooker_conf().exists() {
        return Ok(());
    }
    write_fragment(paths, None)
}

fn ensure_hypr_source(paths: &Paths) -> Result<()> {
    let Some((target, body)) = source_update(paths)? else {
        return Ok(());
    };
    let mut next = body;
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push('\n');
    next.push_str(SOURCE_MARKER);
    next.push('\n');
    next.push_str(SOURCE_LINE);
    next.push('\n');
    atomic_write_fsync(&target, next.as_bytes())
}

fn source_update(paths: &Paths) -> Result<Option<(PathBuf, String)>> {
    let conf = paths.hyprland_conf();
    let body = match fs::read_to_string(&conf) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "{} not found; Rice Cooker cannot persist installs without adding `{SOURCE_LINE}`",
                conf.display()
            )
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", conf.display())),
    };
    if has_active_source(&body) {
        return Ok(None);
    }

    let metadata = fs::metadata(&conf).with_context(|| format!("reading {}", conf.display()))?;
    if metadata.permissions().readonly() {
        bail!(
            "{} is read-only; cannot add `{SOURCE_LINE}`",
            conf.display()
        );
    }

    let target =
        fs::canonicalize(&conf).with_context(|| format!("resolving {}", conf.display()))?;
    Ok(Some((target, body)))
}

fn has_active_source(body: &str) -> bool {
    body.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#')
            && trimmed.split_once('=').is_some_and(|(key, value)| {
                key.trim() == "source" && value.trim() == SOURCE_LINE_PATH
            })
    })
}

fn write_fragment(paths: &Paths, name: Option<&str>) -> Result<()> {
    let body = match name {
        Some(name) => format!(
            "# Managed by Rice Cooker.\nexec-once = sh -c 'pkill -x quickshell 2>/dev/null; sleep 0.1; exec quickshell -c \"$1\"' rice-cooker {}\n",
            shell_quote(name)
        ),
        None => "# Managed by Rice Cooker.\n# no active rice\n".to_string(),
    };
    atomic_write_fsync(&paths.hypr_rice_cooker_conf(), body.as_bytes())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempdir().unwrap();
        let home = t.path().join("home");
        let cache = t.path().join("cache");
        let data = t.path().join("data");
        let paths = Paths::at_roots(home, cache, data);
        fs::create_dir_all(paths.hypr_config_dir()).unwrap();
        (t, paths)
    }

    #[test]
    fn install_writes_fragment_and_source_once() {
        let (_t, paths) = tmp_paths();
        fs::write(paths.hyprland_conf(), "monitor = , preferred, auto, 1\n").unwrap();

        install_hypr_autostart(&paths, "noctalia").unwrap();
        install_hypr_autostart(&paths, "noctalia").unwrap();

        let fragment = fs::read_to_string(paths.hypr_rice_cooker_conf()).unwrap();
        assert!(fragment.contains("exec quickshell -c"));
        assert!(fragment.contains("noctalia"));

        let conf = fs::read_to_string(paths.hyprland_conf()).unwrap();
        assert_eq!(conf.matches(SOURCE_MARKER).count(), 1);
        assert_eq!(conf.matches(SOURCE_LINE).count(), 1);
    }

    #[test]
    fn commented_source_line_is_not_treated_as_active() {
        let (_t, paths) = tmp_paths();
        fs::write(
            paths.hyprland_conf(),
            format!("# {SOURCE_LINE}\nmonitor = , preferred, auto, 1\n"),
        )
        .unwrap();

        install_hypr_autostart(&paths, "noctalia").unwrap();

        let conf = fs::read_to_string(paths.hyprland_conf()).unwrap();
        assert_eq!(conf.matches(SOURCE_LINE).count(), 2);
    }

    #[test]
    fn compact_source_line_is_treated_as_active() {
        let (_t, paths) = tmp_paths();
        fs::write(
            paths.hyprland_conf(),
            format!("source={SOURCE_LINE_PATH}\n"),
        )
        .unwrap();

        install_hypr_autostart(&paths, "noctalia").unwrap();

        let conf = fs::read_to_string(paths.hyprland_conf()).unwrap();
        assert_eq!(conf.matches("rice-cooker.conf").count(), 1);
    }

    #[test]
    fn installing_second_rice_rewrites_fragment() {
        let (_t, paths) = tmp_paths();
        fs::write(paths.hyprland_conf(), "monitor = , preferred, auto, 1\n").unwrap();

        install_hypr_autostart(&paths, "noctalia").unwrap();
        install_hypr_autostart(&paths, "zephyr").unwrap();

        let fragment = fs::read_to_string(paths.hypr_rice_cooker_conf()).unwrap();
        assert!(fragment.contains("zephyr"));
        assert!(!fragment.contains("noctalia"));
    }

    #[test]
    fn clear_writes_inert_fragment() {
        let (_t, paths) = tmp_paths();
        fs::write(
            paths.hypr_rice_cooker_conf(),
            "exec-once = quickshell -c old\n",
        )
        .unwrap();

        clear_hypr_autostart(&paths).unwrap();

        let fragment = fs::read_to_string(paths.hypr_rice_cooker_conf()).unwrap();
        assert_eq!(fragment, "# Managed by Rice Cooker.\n# no active rice\n");
    }

    #[test]
    fn clear_is_noop_without_fragment() {
        let (_t, paths) = tmp_paths();

        clear_hypr_autostart(&paths).unwrap();

        assert!(!paths.hypr_rice_cooker_conf().exists());
    }

    #[test]
    fn missing_hyprland_conf_fails() {
        let (_t, paths) = tmp_paths();

        let err = install_hypr_autostart(&paths, "noctalia").unwrap_err();

        assert!(format!("{err:#}").contains("hyprland.conf"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_hyprland_conf_keeps_symlink_and_updates_target() {
        use std::os::unix::fs::symlink;

        let (_t, paths) = tmp_paths();
        let dotfiles = paths.home.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        let target = dotfiles.join("hyprland.conf");
        fs::write(&target, "input { }\n").unwrap();
        symlink(&target, paths.hyprland_conf()).unwrap();

        install_hypr_autostart(&paths, "noctalia").unwrap();

        assert!(
            fs::symlink_metadata(paths.hyprland_conf())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let final_content = fs::read_to_string(target).unwrap();
        assert!(final_content.contains("input { }"));
        assert!(final_content.contains(SOURCE_LINE));
    }
}
