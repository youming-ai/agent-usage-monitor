//! Self-update: fetch the latest GitHub release, download the asset for this
//! platform, extract it, and atomically replace the running binary.
//!
//! Replaces the previous implementation that shell'd out to `curl` and `tar`
//! — those calls were non-portable and had `to_str().unwrap()` panics on
//! non-UTF-8 paths. The new path uses `ureq` + `tar` + `flate2`, all pure
//! Rust.

mod platform;

use flate2::read::GzDecoder;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO: &str = "youming-ai/agent-usage-monitor";
const BINARY_NAME: &str = "aum";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Every release asset must be served from here — rejects a release-info
/// response that's been tampered with to point at an attacker-controlled
/// host (the GitHub API JSON is technically attacker-influenced if the
/// account or token issuing it is ever compromised; TLS alone only proves
/// *a* server presented a valid cert for the host it's reached).
const EXPECTED_URL_PREFIX: &str =
    "https://github.com/youming-ai/agent-usage-monitor/releases/download/";

/// Hard ceiling on both the raw download and the decompressed tar stream.
/// Real release tarballs are a few MB; this just bounds a corrupted or
/// hostile response (e.g. a decompression bomb) instead of filling the disk.
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

// ponytail: the release signing key. Maintainer must replace this with the
// real public key printed by `minisign -G` before cutting a signed release —
// see install.sh and the release workflow for the matching steps. Until then
// every update will correctly fail signature verification.
const MINISIGN_PUBLIC_KEY: &str =
    "RWQ...PLACEHOLDER_REPLACE_ME_WITH_REAL_MINISIGN_PUBLIC_KEY...";

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

    // Compare versions. `--force` is the only way to install a version that
    // isn't strictly newer, so a compromised/rolled-back release feed can't
    // silently downgrade a user onto a known-vulnerable build.
    if !force && !is_upgrade(&current_ver, &latest_ver) {
        let message = if current_ver == latest_ver {
            "Already on the latest version".to_string()
        } else {
            format!(
                "Installed version v{current_ver} is newer than the latest release v{latest_ver}; \
                 skipping to avoid a downgrade (use --force to override)"
            )
        };
        return Ok(UpdateResult {
            updated: false,
            old_version: current_ver.clone(),
            new_version: latest_ver,
            message,
        });
    }

    // Find the correct asset for current platform
    let asset_name = platform::asset_name()?;
    let asset = find_asset(&release.assets, &asset_name)
        .ok_or_else(|| format!("No release asset found for {asset_name}"))?;
    validate_asset_url(&asset.browser_download_url)?;

    // The signature is mandatory: no `.minisig` asset means we cannot verify
    // the binary before running it, so refuse rather than install blind.
    let sig_asset_name = format!("{asset_name}.minisig");
    let sig_asset = find_asset(&release.assets, &sig_asset_name).ok_or_else(|| {
        format!(
            "Release {} is missing the signature asset {sig_asset_name} — refusing to install \
             an unverifiable binary. This should never happen for an official release; please \
             report it at https://github.com/{REPO}/issues",
            release.tag_name
        )
    })?;
    validate_asset_url(&sig_asset.browser_download_url)?;

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

    download_and_install(
        &asset.browser_download_url,
        &sig_asset.browser_download_url,
        &binary_path,
    )?;

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

fn find_asset<'a>(assets: &'a [Asset], name: &str) -> Option<&'a Asset> {
    assets.iter().find(|a| a.name == name)
}

/// Reject any asset URL that isn't a same-repo GitHub release download —
/// GitHub 302-redirects these to S3 (see `fetch_latest_release`), but the
/// *original* request must hit our own repo, not an attacker-supplied host.
fn validate_asset_url(url: &str) -> Result<(), String> {
    if url.starts_with(EXPECTED_URL_PREFIX) {
        Ok(())
    } else {
        Err(format!(
            "Refusing to download from unexpected URL (expected it to start with \
             {EXPECTED_URL_PREFIX}): {url}"
        ))
    }
}

/// True if `latest` is a strictly greater `major.minor.patch` than `current`.
/// Falls back to a plain inequality check if either string doesn't parse —
/// both are cargo/release-please generated so this should never trigger, but
/// it means a weird version string blocks nothing rather than blocking
/// everything.
fn is_upgrade(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => current != latest,
    }
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Verify `data` against a detached minisign signature using the embedded
/// release public key. `signature_text` is the full contents of a `.minisig`
/// file (untrusted comment + signature + trusted comment lines).
fn verify_signature(data: &[u8], signature_text: &str) -> Result<(), String> {
    let public_key = PublicKey::from_base64(MINISIGN_PUBLIC_KEY)
        .map_err(|e| format!("Invalid embedded minisign public key: {e}"))?;
    let signature = Signature::decode(signature_text)
        .map_err(|e| format!("Malformed .minisig signature: {e}"))?;
    public_key.verify(data, &signature, false).map_err(|e| {
        format!(
            "Signature verification FAILED: the downloaded release does not match the official \
             signature and will NOT be installed. This could mean a corrupted download, a \
             tampered release, or a compromised distribution channel. ({e})"
        )
    })
}

/// Download the tarball, verify it against its detached minisign signature,
/// extract it, and atomically replace `target` with the `aum` binary inside.
/// The new binary is staged as a sibling of `target` so the final swap is a
/// same-filesystem `rename` — atomic, safe to run on the live binary (the
/// kernel keeps the old inode alive until exit), and free of the EXDEV
/// failure a direct rename from the OS tempdir would hit.
fn download_and_install(url: &str, sig_url: &str, target: &Path) -> Result<(), String> {
    let tmp_dir =
        tempfile::tempdir().map_err(|e| format!("Failed to create temp directory: {e}"))?;
    let archive_path = tmp_dir.path().join("update.tar.gz");

    println!("Downloading update...");
    download_to_path(url, &archive_path).map_err(|e| format!("Download failed: {e}"))?;

    println!("Downloading signature...");
    let signature_text = download_text(sig_url).map_err(|e| format!("Signature download failed: {e}"))?;

    println!("Verifying signature...");
    let archive_bytes =
        std::fs::read(&archive_path).map_err(|e| format!("Failed to read downloaded archive: {e}"))?;
    verify_signature(&archive_bytes, &signature_text)?;
    drop(archive_bytes);

    println!("Extracting...");
    let archive_file =
        std::fs::File::open(&archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let decoder = GzDecoder::new(archive_file);
    // Cap decompressed output too, not just the download — a small malicious
    // gzip can still expand to gigabytes (a "decompression bomb").
    let capped_decoder = CappedReader::new(decoder, MAX_DOWNLOAD_BYTES);
    let mut archive = tar::Archive::new(capped_decoder);
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
    let reader = ureq::get(url)
        .set("User-Agent", "aum-self-updater")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| io::Error::other(format!("{e}")))?
        .into_reader();
    let mut capped = CappedReader::new(reader, MAX_DOWNLOAD_BYTES);
    let mut file = std::fs::File::create(dest)?;
    io::copy(&mut capped, &mut file)?;
    Ok(())
}

/// Download a small text asset (used for the `.minisig` signature file).
fn download_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .set("User-Agent", "aum-self-updater")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| format!("{e}"))?
        .into_string()
        .map_err(|e| format!("{e}"))
}

/// Reader adapter that errors once more than `limit` bytes have been read.
/// Used to cap both the raw tarball download and the decompressed tar
/// stream, so a corrupted or hostile response can't fill the disk (a
/// decompression bomb is just an extreme case of the latter).
struct CappedReader<R> {
    inner: R,
    remaining: u64,
    limit: u64,
}

impl<R: io::Read> CappedReader<R> {
    fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
            limit,
        }
    }
}

impl<R: io::Read> io::Read for CappedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            // Already at the cap: only an error if the source still has more
            // data to give (as opposed to us landing exactly on a real EOF).
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe) {
                Ok(0) => Ok(0),
                Ok(_) => Err(io::Error::other(format!(
                    "exceeds the {}-byte safety cap",
                    self.limit
                ))),
                Err(e) => Err(e),
            };
        }
        let want = buf.len().min(self.remaining as usize);
        let n = self.inner.read(&mut buf[..want])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated locally with `minisign -G` for this test only — NOT the
    // release signing key. Keep the secret key out of the repo entirely;
    // only the public key and a pre-made signature are needed to test
    // verification.
    const TEST_PUBLIC_KEY: &str = "RWQzDUci4432ZjUnkhyJh5pNeFTcueRPNTo2kEVeqeYK39jniDuDIRpm";
    const TEST_MESSAGE: &[u8] = b"test message for minisign-verify unit test\n";
    const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
RUQzDUci4432ZkMygRu1gwErLTNfqrcqckUC5LVC4aqrEdd4RsVv1iGTF1wqZl79AE6muyXcdBTYcfZs+MoVq1EMAat0vYTAlgE=\n\
trusted comment: timestamp:1784194586\tfile:msg.txt\thashed\n\
3ETikRhLVipT/lxcvZpSQN3UPeLXei+LJqOJbVG5NUtjoowNGr+Rg7wW74XDl2ofHKQsfFkFAVtYs1l2d7JGDQ==\n";

    fn verify_with_key(public_key: &str, data: &[u8], signature_text: &str) -> Result<(), String> {
        let public_key = PublicKey::from_base64(public_key)
            .map_err(|e| format!("Invalid embedded minisign public key: {e}"))?;
        let signature =
            Signature::decode(signature_text).map_err(|e| format!("Malformed .minisig signature: {e}"))?;
        public_key
            .verify(data, &signature, false)
            .map_err(|e| format!("verification failed: {e}"))
    }

    #[test]
    fn accepts_a_correctly_signed_payload() {
        verify_with_key(TEST_PUBLIC_KEY, TEST_MESSAGE, TEST_SIGNATURE)
            .expect("valid signature over the exact signed message must verify");
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let tampered = b"test message for minisign-verify unit test! (modified)\n";
        let err = verify_with_key(TEST_PUBLIC_KEY, tampered, TEST_SIGNATURE)
            .expect_err("signature must not verify against different data");
        assert!(err.contains("verification failed"));
    }

    #[test]
    fn rejects_a_signature_from_a_different_key() {
        // A second, unrelated locally-generated test keypair (not the one
        // TEST_SIGNATURE was made with, and not the release key).
        let other_key = "RWTPrta1vDhRy1CIryLxYHz5Kpldz8PHukQWGTbS7zHGsW0+97K6pB65";
        let err = verify_with_key(other_key, TEST_MESSAGE, TEST_SIGNATURE)
            .expect_err("signature must not verify against an unrelated public key");
        assert!(err.contains("verification failed"));
    }

    #[test]
    fn is_upgrade_rejects_equal_and_lower_versions() {
        assert!(is_upgrade("1.2.3", "1.2.4"));
        assert!(is_upgrade("1.2.3", "1.3.0"));
        assert!(!is_upgrade("1.2.3", "1.2.3"));
        assert!(!is_upgrade("1.2.3", "1.2.2"));
        assert!(!is_upgrade("2.0.0", "1.9.9"));
    }

    #[test]
    fn capped_reader_allows_exact_limit() {
        let data = vec![7u8; 10];
        let mut capped = CappedReader::new(io::Cursor::new(data.clone()), 10);
        let mut out = Vec::new();
        io::copy(&mut capped, &mut out).expect("reading exactly the cap must succeed");
        assert_eq!(out, data);
    }

    #[test]
    fn capped_reader_rejects_oversized_input() {
        let data = vec![7u8; 11];
        let mut capped = CappedReader::new(io::Cursor::new(data), 10);
        let mut out = Vec::new();
        let err = io::copy(&mut capped, &mut out).expect_err("exceeding the cap must error");
        assert!(err.to_string().contains("safety cap"));
    }
}
