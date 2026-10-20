//! Thin wrappers around `std::process::Command`. Nothing here goes through a
//! shell unless the caller asks for `sh -c` explicitly.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Run a program and return its stdout. A non-zero exit becomes an error
/// carrying stderr (or stdout when stderr is empty).
pub fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            if stdout.trim().is_empty() {
                format!("{program} exited with {}", output.status)
            } else {
                stdout.trim().to_string()
            }
        } else {
            stderr
        })
    }
}

/// Run a program, feeding `input` on stdin.
pub fn run_with_stdin(program: &str, args: &[&str], input: &[u8]) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input)
            .map_err(|e| format!("{program}: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            stderr
        })
    }
}

/// Start a program without waiting for it. It gets its own process group
/// so it survives this app closing.
pub fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{program}: {e}"))
}

/// Is `program` on `$PATH` (or an existing absolute path)?
pub fn exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

/// Write a root-owned file through `pkexec`, keeping a `.bak` copy of the
/// previous contents next to it. One authentication prompt.
pub fn pkexec_write(path: &Path, contents: &str) -> Result<(), String> {
    let target = path.to_string_lossy();
    let script = r#"set -e; [ -f "$1" ] && cp -a "$1" "$1.bak"; cat > "$1""#;
    run_with_stdin(
        "pkexec",
        &["sh", "-c", script, "m3-settings", &target],
        contents.as_bytes(),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_reports_failures() {
        assert_eq!(run("echo", &["hi"]).unwrap().trim(), "hi");
        assert!(run("false", &[]).is_err());
        assert!(run("definitely-not-a-real-program-m3s", &[]).is_err());
    }

    #[test]
    fn stdin_is_fed() {
        assert_eq!(run_with_stdin("cat", &[], b"abc").unwrap(), "abc");
    }

    #[test]
    fn path_lookup() {
        assert!(exists("sh"));
        assert!(!exists("definitely-not-a-real-program-m3s"));
    }
}
