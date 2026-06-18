/// Get the asset name for the current platform
pub fn asset_name() -> Result<String, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let os_name = match os {
        "macos" => "darwin",
        "linux" => "linux",
        _ => return Err(format!("Unsupported OS: {}", os)),
    };

    let arch_name = match arch {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        _ => return Err(format!("Unsupported architecture: {}", arch)),
    };

    Ok(format!("aum-{}-{}.tar.gz", os_name, arch_name))
}
