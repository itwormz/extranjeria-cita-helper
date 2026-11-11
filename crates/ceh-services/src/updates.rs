//! Check the public GitHub releases of the project for a newer stable version. Only the check: the person
//! opens the release page; nothing is downloaded or installed here.

const API: &str = "https://api.github.com/repos/itwormz/santacita/releases/latest";
const PAGE_PREFIX: &str = "https://github.com/itwormz/santacita/releases/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    Invalid,
    Network,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Latest {
    pub version: String,
    pub url: String,
}
fn numbers(version: &str) -> Option<[u64; 3]> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let found = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(found)
}
/// The stable release named by a GitHub "latest release" answer.
pub fn parse_latest(body: &str) -> Result<Latest, UpdateError> {
    let raw: serde_json::Value = serde_json::from_str(body).map_err(|_| UpdateError::Invalid)?;
    let tag = raw["tag_name"].as_str().ok_or(UpdateError::Invalid)?;
    let url = raw["html_url"].as_str().ok_or(UpdateError::Invalid)?;
    let version = tag.strip_prefix('v').ok_or(UpdateError::Invalid)?;
    let stable = raw["draft"] != true && raw["prerelease"] != true && !version.contains(['-', '+']);
    if !stable || numbers(version).is_none() || !url.starts_with(PAGE_PREFIX) || url.len() > 300 {
        return Err(UpdateError::Invalid);
    }
    Ok(Latest { version: version.to_owned(), url: url.to_owned() })
}
/// Whether `latest` is newer than `current`; a development build (`1.0.0-dev.3`) is older than the release of
/// its own number.
pub fn newer(latest: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (numbers(latest), numbers(current)) else { return false };
    a > b || (a == b && current.contains("-dev"))
}
/// Ask GitHub for the latest release (blocking, bounded, no credentials).
pub fn fetch_latest() -> Result<Latest, UpdateError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("SantaCita")
        .https_only(true)
        .build()
        .map_err(|_| UpdateError::Network)?;
    let response = client
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| UpdateError::Network)?;
    let body = response.bytes().map_err(|_| UpdateError::Network)?;
    if body.len() > 1024 * 1024 {
        return Err(UpdateError::Invalid);
    }
    parse_latest(std::str::from_utf8(&body).map_err(|_| UpdateError::Invalid)?)
}

/// The system and CPU the running app was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub system: &'static str,
    pub arch: &'static str,
}
impl Target {
    pub fn current() -> Option<Self> {
        let system = if cfg!(target_os = "macos") { "macos" } else if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "linux") { "linux" } else { return None };
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else if cfg!(target_arch = "x86_64") { "x64" } else { return None };
        // Only the combinations the project publishes.
        matches!((system, arch), ("macos", _) | ("windows", "x64") | ("linux", "x64")).then_some(Self { system, arch })
    }
}
/// A package of a stable release, with the files that vouch for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub name: String,
    pub url: String,
    pub sums_url: String,
    pub size: u64,
}
pub const MAX_PACKAGE: u64 = 2 * 1024 * 1024 * 1024;
const DOWNLOAD_PREFIX: &str = "https://github.com/itwormz/santacita/releases/download/";
fn package_names(version: &str, target: Target) -> Vec<String> {
    match (target.system, target.arch) {
        ("macos", arch) => {
            let chip = if arch == "arm64" { "Apple-Silicon" } else { "Intel" };
            vec![format!("SantaCita-{version}-macOS-{chip}.dmg"), format!("extranjeria-cita-helper-{version}-macos-{arch}.dmg")]
        }
        ("windows", arch) => vec![format!("SantaCita-{version}-windows-{arch}.msi"), format!("extranjeria-cita-helper-{version}-windows-{arch}.msi")],
        _ => vec![format!("SantaCita-{version}-x86_64.AppImage"), format!("Cita_Extranjeria_Helper-{version}-x86_64.AppImage")],
    }
}
/// The package for `target` when the latest release is newer than `current`; `None` when it is not.
/// The release must be immutable, carry a `SHA256SUMS` file, and every file must be the project's own
/// GitHub release asset.
pub fn select_release(body: &str, current: &str, target: Target) -> Result<Option<Release>, UpdateError> {
    let latest = parse_latest(body)?;
    if !newer(&latest.version, current) {
        return Ok(None);
    }
    let raw: serde_json::Value = serde_json::from_str(body).map_err(|_| UpdateError::Invalid)?;
    if raw["immutable"] != true {
        return Err(UpdateError::Invalid);
    }
    let base = format!("{DOWNLOAD_PREFIX}v{}/", latest.version);
    let assets = raw["assets"].as_array().ok_or(UpdateError::Invalid)?;
    let asset = |name: &str| -> Option<(String, u64)> {
        let mut found = assets.iter().filter(|a| a["name"] == name);
        let first = found.next()?;
        let url = first["browser_download_url"].as_str()?;
        (found.next().is_none() && url == format!("{base}{name}")).then(|| (url.to_owned(), first["size"].as_u64().unwrap_or(0)))
    };
    let (name, (url, size)) = package_names(&latest.version, target)
        .into_iter()
        .find_map(|name| asset(&name).map(|found| (name, found)))
        .ok_or(UpdateError::Invalid)?;
    let (sums_url, _) = asset("SHA256SUMS").ok_or(UpdateError::Invalid)?;
    if size == 0 || size > MAX_PACKAGE {
        return Err(UpdateError::Invalid);
    }
    Ok(Some(Release { version: latest.version, name, url, sums_url, size }))
}
/// The checksum of `name` in a `SHA256SUMS` file; the file must name it exactly once.
pub fn checksum_for(text: &str, name: &str) -> Result<String, UpdateError> {
    let mut found = text.lines().filter_map(|line| {
        let (digest, file) = line.split_once("  ")?;
        (digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()) && file == name).then(|| digest.to_ascii_lowercase())
    });
    let first = found.next().ok_or(UpdateError::Invalid)?;
    found.next().is_none().then_some(first).ok_or(UpdateError::Invalid)
}
/// Write `reader` to `<dir>/<name>` only when it has exactly `size` bytes and the SHA-256 `digest`; the
/// checksum is stored next to it. A cancel, a short or long stream or a mismatch leaves nothing behind.
pub fn stream_verified(
    mut reader: impl std::io::Read,
    size: u64,
    digest: &str,
    name: &str,
    dir: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(u8),
) -> Result<std::path::PathBuf, UpdateError> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::sync::atomic::Ordering;
    if name.is_empty() || name.contains(['/', '\\']) || size == 0 || size > MAX_PACKAGE || digest.len() != 64 {
        return Err(UpdateError::Invalid);
    }
    std::fs::create_dir_all(dir).map_err(|_| UpdateError::Invalid)?;
    let (destination, partial) = (dir.join(name), dir.join(format!("{name}.part")));
    let result = (|| {
        let mut output = std::fs::File::create(&partial).map_err(|_| UpdateError::Invalid)?;
        let (mut hash, mut count) = (Sha256::new(), 0u64);
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(UpdateError::Cancelled);
            }
            let read = reader.read(&mut buffer).map_err(|_| UpdateError::Network)?;
            if read == 0 {
                break;
            }
            count += read as u64;
            if count > size {
                return Err(UpdateError::Invalid);
            }
            hash.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(|_| UpdateError::Invalid)?;
            progress((count * 100 / size).min(100) as u8);
        }
        if count != size || format!("{:x}", hash.finalize()) != digest.to_ascii_lowercase() {
            return Err(UpdateError::Invalid);
        }
        output.flush().map_err(|_| UpdateError::Invalid)?;
        std::fs::rename(&partial, &destination).map_err(|_| UpdateError::Invalid)?;
        std::fs::write(dir.join(format!("{name}.sha256")), format!("{digest}  {name}\n")).map_err(|_| UpdateError::Invalid)?;
        Ok(destination.clone())
    })();
    let _ = std::fs::remove_file(&partial);
    result
}
/// Fetch the latest release, pick the package, download and verify it (checksum from the release's own
/// `SHA256SUMS`). Provenance (Sigstore) is not checked yet: it depends on the release workflow of this app.
pub fn download_latest(
    current: &str,
    dir: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: impl FnMut(u8),
) -> Result<std::path::PathBuf, UpdateError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(3600))
        .user_agent("SantaCita")
        .https_only(true)
        .build()
        .map_err(|_| UpdateError::Network)?;
    let get = |url: &str, accept: &str| client.get(url).header("Accept", accept).send().and_then(reqwest::blocking::Response::error_for_status).map_err(|_| UpdateError::Network);
    let body = get(API, "application/vnd.github+json")?.text().map_err(|_| UpdateError::Network)?;
    if body.len() > 1024 * 1024 {
        return Err(UpdateError::Invalid);
    }
    let target = Target::current().ok_or(UpdateError::Invalid)?;
    let release = select_release(&body, current, target)?.ok_or(UpdateError::Invalid)?;
    let sums = get(&release.sums_url, "application/octet-stream")?.text().map_err(|_| UpdateError::Network)?;
    if sums.len() > 16 * 1024 {
        return Err(UpdateError::Invalid);
    }
    let digest = checksum_for(&sums, &release.name)?;
    let response = get(&release.url, "application/octet-stream")?;
    stream_verified(response, release.size, &digest, &release.name, dir, cancel, progress)
}
