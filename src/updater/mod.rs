mod platform;

use serde::Deserialize;
use std::path::{Path, PathBuf};

const REPO: &str = "youming-ai/agent-usage-monitor";
const BINARY_NAME: &str = "aum";

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
    let asset = release.assets.iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| format!("No release asset found for {}", asset_name))?;
    
    if dry_run {
        return Ok(UpdateResult {
            updated: false,
            old_version: current_ver,
            new_version: latest_ver,
            message: format!("Would download: {}", asset.browser_download_url),
        });
    }
    
    // Download and install
    let binary_path = current_binary_path()
        .ok_or("Could not determine current binary path")?;
    
    download_and_install(&asset.browser_download_url, &binary_path)?;
    
    Ok(UpdateResult {
        updated: true,
        old_version: current_ver,
        new_version: latest_ver,
        message: format!("Successfully updated to v{}", release.tag_name.trim_start_matches('v')),
    })
}

/// Fetch latest release info from GitHub API
fn fetch_latest_release() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    
    let output = std::process::Command::new("curl")
        .args(["-s", "-H", "Accept: application/vnd.github.v3+json", &url])
        .output()
        .map_err(|e| format!("Failed to execute curl: {}. Is curl installed?", e))?;
    
    if !output.status.success() {
        return Err("Failed to fetch release info".to_string());
    }
    
    let body = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse release info: {}", e))
}

/// Download and install the new binary
fn download_and_install(url: &str, target_path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    
    // Create a temporary directory
    let tmp_dir = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;
    
    let archive_path = tmp_dir.path().join("update.tar.gz");
    
    // Download the archive
    println!("Downloading update...");
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o", archive_path.to_str().unwrap(), url])
        .status()
        .map_err(|e| format!("Failed to download: {}", e))?;
    
    if !status.success() {
        return Err("Download failed".to_string());
    }
    
    // Extract the archive
    println!("Extracting...");
    let status = std::process::Command::new("tar")
        .args(["xzf", archive_path.to_str().unwrap(), "-C", tmp_dir.path().to_str().unwrap()])
        .status()
        .map_err(|e| format!("Failed to extract: {}", e))?;
    
    if !status.success() {
        return Err("Extraction failed".to_string());
    }
    
    let new_binary = tmp_dir.path().join(BINARY_NAME);
    
    // Check if we need sudo
    let needs_sudo = !is_writable(target_path);
    
    // Install the new binary
    println!("Installing to {}...", target_path.display());
    if needs_sudo {
        let status = std::process::Command::new("sudo")
            .args(["mv", new_binary.to_str().unwrap(), target_path.to_str().unwrap()])
            .status()
            .map_err(|e| format!("Failed to install with sudo: {}", e))?;
        
        if !status.success() {
            return Err("Installation failed".to_string());
        }
        
        let status = std::process::Command::new("sudo")
            .args(["chmod", "755", target_path.to_str().unwrap()])
            .status()
            .map_err(|e| format!("Failed to set permissions: {}", e))?;
        
        if !status.success() {
            return Err("Failed to set permissions".to_string());
        }
    } else {
        std::fs::copy(&new_binary, target_path)
            .map_err(|e| format!("Failed to copy binary: {}", e))?;
        
        std::fs::set_permissions(target_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set permissions: {}", e))?;
    }
    
    Ok(())
}

/// Check if a path is writable
fn is_writable(path: &Path) -> bool {
    if let Some(parent) = path.parent()
        && let Ok(metadata) = std::fs::metadata(parent) {
            use std::os::unix::fs::MetadataExt;
            // Check if we own the directory or have write permission
            let uid = unsafe { libc::getuid() };
            return metadata.uid() == uid || (metadata.mode() & 0o002) != 0;
        }
    false
}
