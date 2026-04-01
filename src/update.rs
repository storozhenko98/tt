use std::fs;

const REPO: &str = "storozhenko98/tt";

fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("tt-darwin-arm64"),
        ("linux", "x86_64") => Some("tt-linux-x64"),
        _ => None,
    }
}

/// Fetch the latest release tag from GitHub (no `gh` needed, just curl).
fn latest_tag() -> Option<String> {
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/{}/releases/latest", REPO),
        ])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&out.stdout);
    // Minimal JSON parsing — find "tag_name": "v..."
    let marker = "\"tag_name\":";
    let idx = body.find(marker)? + marker.len();
    let rest = &body[idx..];
    let start = rest.find('"')? + 1;
    let end = start + rest[start..].find('"')?;
    Some(rest[start..end].to_string())
}

/// Compare semver: returns true if remote > current.
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let r = parse(remote);
    let c = parse(current);
    for i in 0..r.len().max(c.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if rv > cv {
            return true;
        }
        if rv < cv {
            return false;
        }
    }
    false
}

/// Check for updates and self-replace if a newer version exists.
/// Prints a one-liner and restarts via exec. Returns normally if
/// already up-to-date or if the update fails (game continues either way).
pub fn auto_update() {
    let asset = match asset_name() {
        Some(a) => a,
        None => return,
    };

    let tag = match latest_tag() {
        Some(t) => t,
        None => return, // offline or rate-limited, skip silently
    };

    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    if !is_newer(&tag, &current) {
        return; // already latest
    }

    eprintln!("Updating tt {} -> {} ...", current, tag);

    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        REPO, tag, asset
    );

    let tmp = std::env::temp_dir().join("tt-update");
    let ok = std::process::Command::new("curl")
        .args(["-fsSL", "-o", &tmp.to_string_lossy(), &url])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        eprintln!("Update download failed, continuing with current version.");
        return;
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };

    // Replace binary
    let replaced = fs::rename(&tmp, &exe)
        .or_else(|_| fs::copy(&tmp, &exe).map(|_| ()))
        .is_ok();

    let _ = fs::remove_file(&tmp);

    if replaced {
        eprintln!("Updated to {}. Restarting...", tag);
        // Re-exec ourselves with the same args
        let args: Vec<String> = std::env::args().collect();
        let err = exec(&exe, &args);
        eprintln!("Restart failed: {}", err);
    }
}

/// Replace the current process (Unix exec).
#[cfg(unix)]
fn exec(exe: &std::path::Path, args: &[String]) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    std::process::Command::new(exe).args(&args[1..]).exec()
}

#[cfg(not(unix))]
fn exec(_exe: &std::path::Path, _args: &[String]) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, "not unix")
}
