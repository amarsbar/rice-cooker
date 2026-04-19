use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn preflight() -> anyhow::Result<()> {
    let status = Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => anyhow::bail!("git is required but returned a non-zero exit code"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            anyhow::bail!("git is required but was not found on PATH")
        }
        Err(e) => anyhow::bail!("git is required but could not be launched: {}", e),
    }
}

pub fn clone_or_update(repo_url: &str, dest: &Path, log_file: &Path) -> anyhow::Result<()> {
    // Reject URLs starting with `-` up front: our message is clearer than whatever
    // git would emit and the `--` separator below only guards the top-level URL,
    // not e.g. submodule URLs. The `ext::` vector is separately blocked by the
    // `-c protocol.ext.allow=never` config in git_cmd() — no need to double-check here.
    if repo_url.starts_with('-') {
        anyhow::bail!("refusing repo URL starting with '-': {}", repo_url);
    }

    if dest.join(".git").exists() {
        let fetch_status = git_cmd()
            .args(["-C"])
            .arg(dest)
            .args(["fetch", "--depth", "1", "origin", "HEAD"])
            .stderr(open_log(log_file)?)
            .status()?;

        if !fetch_status.success() {
            anyhow::bail!("git fetch failed with exit code {:?}", fetch_status.code());
        }

        let reset_status = git_cmd()
            .args(["-C"])
            .arg(dest)
            .args(["reset", "--hard", "FETCH_HEAD"])
            .stderr(open_log(log_file)?)
            .status()?;

        if !reset_status.success() {
            anyhow::bail!("git reset failed with exit code {:?}", reset_status.code());
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        // `--` separates options from positional arguments; without it, a repo URL starting
        // with `-` (e.g. `--upload-pack=...`) would be interpreted as a git option — a
        // known RCE vector for git wrappers.
        let clone_status = git_cmd()
            .args(["clone", "--depth", "1", "--", repo_url])
            .arg(dest)
            .stderr(open_log(log_file)?)
            .status()?;

        if !clone_status.success() {
            anyhow::bail!("git clone failed with exit code {:?}", clone_status.code());
        }
    }

    Ok(())
}

// Preconfigured `git` invocation with hardening env + config:
// - GIT_TERMINAL_PROMPT=0 / GIT_ASKPASS=/bin/false keep git from blocking on a
//   credential prompt when spawned from a GUI with no attached terminal.
// - DISPLAY="" suppresses ssh's ASKPASS mechanism (ssh only invokes SSH_ASKPASS
//   when DISPLAY is set), so an unset DISPLAY alone is sufficient.
// - `-c protocol.ext.allow=never` forbids the `ext::` protocol (arbitrary-
//   command RCE) even if a submodule or remote helper tries to reach for it.
fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "protocol.ext.allow=never"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/bin/false")
        .env("DISPLAY", "");
    cmd
}

fn open_log(log_file: &Path) -> anyhow::Result<fs::File> {
    fs::File::options()
        .create(true)
        .append(true)
        .open(log_file)
        .map_err(|e| anyhow::anyhow!("opening git log {}: {}", log_file.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_bare_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir");
        let work = dir.path().join("work");
        let bare = dir.path().join("repo.git");

        // Init a normal repo, configure identity, commit an empty file.
        let run = |args: &[&str], cwd: &Path| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap_or_else(|_| panic!("git {:?} failed to spawn", args))
                .success();
            assert!(ok, "git {:?} in {:?} failed", args, cwd);
        };

        fs::create_dir_all(&work).expect("create work dir");
        run(&["init"], &work);
        run(&["config", "user.email", "test@example.com"], &work);
        run(&["config", "user.name", "Test"], &work);
        // Create an initial commit so HEAD exists.
        fs::write(work.join("README"), b"rice").expect("write README");
        run(&["add", "."], &work);
        run(&["commit", "-m", "init"], &work);

        // Clone into a bare repo — this becomes the "remote".
        let ok = Command::new("git")
            .args(["clone", "--bare"])
            .arg(&work)
            .arg(&bare)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git clone --bare")
            .success();
        assert!(ok, "git clone --bare failed");

        (dir, bare)
    }

    #[test]
    fn preflight_returns_ok_when_git_available() {
        assert!(preflight().is_ok());
    }

    #[test]
    fn clone_into_empty_dest_creates_git_dir() {
        let (_bare_guard, bare_repo) = make_bare_repo();
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");
        let log_dir = tempdir().expect("log tempdir");
        let log_file = log_dir.path().join("git.log");

        clone_or_update(bare_repo.to_str().unwrap(), &dest, &log_file)
            .expect("clone_or_update should succeed");

        assert!(
            dest.join(".git").exists(),
            ".git directory should exist after clone"
        );
    }

    #[test]
    fn clone_with_invalid_url_returns_err_and_log_populated() {
        let dest_dir = tempdir().expect("dest tempdir");
        let dest = dest_dir.path().join("clone");
        let log_dir = tempdir().expect("log tempdir");
        let log_file = log_dir.path().join("git.log");

        let result = clone_or_update("/no/such/path.git", &dest, &log_file);

        assert!(result.is_err(), "clone of bogus URL should fail");
        let log_len = fs::metadata(&log_file).map(|m| m.len()).unwrap_or(0);
        assert!(
            log_len > 0,
            "log file should be non-empty after failed clone"
        );
    }
}
