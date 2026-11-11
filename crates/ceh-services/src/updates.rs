//! Check the public GitHub releases of the project for a newer stable version. Only the check: the person
//! opens the release page; nothing is downloaded or installed here.

const API: &str = "https://api.github.com/repos/itwormz/extranjeria-cita-helper/releases/latest";
const PAGE_PREFIX: &str = "https://github.com/itwormz/extranjeria-cita-helper/releases/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    Invalid,
    Network,
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
