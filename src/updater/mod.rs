//! Self-update: fetch the latest GitHub release, download the asset for this
//! platform, extract it, and atomically replace the running binary.
//!
//! Replaces the previous implementation that shell'd out to `curl` and `tar`
//! — those calls were non-portable and had `to_str().unwrap()` panics on
//! non-UTF-8 paths. The new path uses `ureq` + `tar` + `flate2`, all pure
//! Rust.

mod platform;

use flate2::read::GzDecoder;
use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "youming-ai/agent-usage-monitor";
const BINARY_NAME: &str = "aum";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
pub struct UpdateResult {
    pub updated: bool,
    pub old_version: String,
    pub new_version: String,
    pub message: String,
}

/// Get the current version of the binary
pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the path where the current binary is installed
pub fn current_binary_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// Check for updates and optionally install
pub fn check_and_update(force: bool, dry_run: bool) -> Result<UpdateResult, String> {
    let current_ver = current_version();

    // Get latest release info
    let release = fetch_latest_release()?;
    let latest_ver = release.tag_name.trim_start_matches('v').to_string();

    // Compare versions
    if !force && current_ver == latest_ver {
        return Ok(UpdateResult {
            updated: false,
            old_version: current_ver.clone(),
            new_version: latest_ver,
            message: "Already on the latest version".to_string(),
        });
    }

    // Find the correct asset for current platform
    let asset_name = platform::asset_name()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| format!("No release asset found for {asset_name}"))?;

    if dry_run {
        return Ok(UpdateResult {
            updated: false,
            old_version: current_ver,
            new_version: latest_ver,
            message: format!("Would download: {}", asset.browser_download_url),
        });
    }

    // Download and install
    let binary_path = current_binary_path().ok_or("Could not determine current binary path")?;

    download_and_install(&asset.browser_download_url, &binary_path)?;

    Ok(UpdateResult {
        updated: true,
        old_version: current_ver,
        new_version: latest_ver,
        message: format!(
            "Successfully updated to v{}",
            release.tag_name.trim_start_matches('v')
        ),
    })
}

/// Fetch latest release info from GitHub API. GitHub requires a User-Agent
/// header, and returns 302s to S3 for asset downloads — ureq follows both.
fn fetch_latest_release() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");

    let body = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", "aum-self-updater")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| format!("Failed to fetch release info: {e}. Check your network connection."))?
        .into_string()
        .map_err(|e| format!("Failed to read release response: {e}"))?;

    serde_json::from_str(&body).map_err(|e| format!("Failed to parse release info: {e}"))
}

/// Download the tarball, extract it, and atomically replace `target` with the
/// `aum` binary inside. The new binary is staged as a sibling of `target` so
/// the final swap is a same-filesystem `rename` — atomic, safe to run on the
/// live binary (the kernel keeps the old inode alive until exit), and free of
/// the EXDEV failure a direct rename from the OS tempdir would hit.
fn download_and_install(url: &str, target: &Path) -> Result<(), String> {
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp directory: {e}"))?;
    let archive_path = tmp_dir.path().join("update.tar.gz");

    println!("Downloading update...");
    download_to_path(url, &archive_path)
        .map_err(|e| format!("Download failed: {e}"))?;

    println!("Extracting...");
    let archive_file = std::fs::File::open(&archive_path)
        .map_err(|e| format!("Failed to open archive: {e}"))?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(tmp_dir.path())
        .map_err(|e| format!("Extraction failed: {e}"))?;

    let new_binary = tmp_dir.path().join(BINARY_NAME);
    if !new_binary.exists() {
        return Err(format!(
            "Archive did not contain a `{BINARY_NAME}` binary at the top level"
        ));
    }

    println!("Installing to {}...", target.display());
    // Stage the new binary as a sibling of the target, then atomically rename
    // it into place. Renaming from the OS tempdir directly would fail with
    // EXDEV when $TMPDIR is on a different mount (the common Linux layout), and
    // copying *onto* the running binary would fail with ETXTBSY on Linux —
    // staging next to the target then renaming avoids both.
    let target_dir = target
        .parent()
        .ok_or_else(|| format!("Cannot determine parent directory of {}", target.display()))?;
    let staged = target_dir.join(format!(".{BINARY_NAME}.update.tmp"));

    // Remove any stale staged file from a previous interrupted run first. A
    // foreign-owned leftover (e.g. from a killed `sudo aum update`) would make
    // the copy below fail with EACCES and be misreported as "needs sudo" — but
    // unlink permission depends on the (writable) directory, not the file, so
    // removing it first succeeds. Clean up again if the copy itself fails.
    let _ = std::fs::remove_file(&staged);
    if let Err(e) = std::fs::copy(&new_binary, &staged) {
        let _ = std::fs::remove_file(&staged);
        return Err(install_error(target, &e));
    }
    // Ensure the staged binary is executable; some tar archives / filesystems
    // don't preserve the +x bit, so set it explicitly before the swap.
    if let Err(e) = set_executable(&staged) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("Failed to set executable bit: {e}"));
    }
    if let Err(e) = std::fs::rename(&staged, target) {
        let _ = std::fs::remove_file(&staged);
        return Err(install_error(target, &e));
    }

    Ok(())
}

/// Build an actionable install error, only pointing at `sudo` / a user-writable
/// directory for a genuine permission failure — so we don't misdirect users
/// there for, e.g., a full disk or other IO error.
fn install_error(target: &Path, e: &io::Error) -> String {
    let base = format!("Failed to install to {}: {e}", target.display());
    if e.kind() == io::ErrorKind::PermissionDenied {
        format!(
            "{base}. The location needs elevated permissions — run `sudo aum update` \
             or install to a user-writable directory like ~/.local/bin"
        )
    } else {
        base
    }
}

fn download_to_path(url: &str, dest: &Path) -> io::Result<()> {
    let mut reader = ureq::get(url)
        .set("User-Agent", "aum-self-updater")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| io::Error::other(format!("{e}")))?
        .into_reader();
    let mut file = std::fs::File::create(dest)?;
    io::copy(&mut reader, &mut file)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}
