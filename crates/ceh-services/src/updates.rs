//! Public releases, optional beta channel and verified native package downloads.
use crate::release_versions::version_key;
use serde_json::Value;
use std::io::Read;
pub const REPOSITORIES: [&str;2] = ["https://github.com/itwormz/santacita", "https://github.com/itwormz/extranjeria-cita-helper"];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError { Invalid, Network, Cancelled }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Latest { pub version: String, pub url: String, pub repository: String }
pub fn api_url(repository: &str, suffix: &str) -> Result<String,UpdateError> {
    if !REPOSITORIES.contains(&repository) { return Err(UpdateError::Invalid); }
    Ok(format!("https://api.github.com/repos/{}{suffix}",repository.strip_prefix("https://github.com/").ok_or(UpdateError::Invalid)?))
}
fn parse_entry(raw: &Value, include_beta: bool, expected: Option<&str>) -> Result<Latest,UpdateError> {
    let tag=raw["tag_name"].as_str().ok_or(UpdateError::Invalid)?;
    let version=tag.strip_prefix('v').ok_or(UpdateError::Invalid)?;
    let key=version_key(version).ok_or(UpdateError::Invalid)?;
    let beta=key.1==1;
    if raw["draft"]==true || raw["prerelease"].as_bool().unwrap_or(false)!=beta || (beta&&!include_beta) { return Err(UpdateError::Invalid); }
    let url=raw["html_url"].as_str().ok_or(UpdateError::Invalid)?;
    let repository=REPOSITORIES.into_iter().find(|repo| url==format!("{repo}/releases/tag/{tag}")).ok_or(UpdateError::Invalid)?;
    if expected.is_some_and(|r|r!=repository) { return Err(UpdateError::Invalid); }
    Ok(Latest{version:version.into(),url:url.into(),repository:repository.into()})
}
pub fn parse_latest(body: &str) -> Result<Latest,UpdateError> {
    parse_entry(&serde_json::from_str(body).map_err(|_|UpdateError::Invalid)?,false,None)
}
pub fn parse_release_list(body: &str, include_beta: bool, repository: &str) -> Result<Option<Latest>,UpdateError> {
    let raw:Value=serde_json::from_str(body).map_err(|_|UpdateError::Invalid)?;
    Ok(raw.as_array().ok_or(UpdateError::Invalid)?.iter().filter_map(|raw|parse_entry(raw,include_beta,Some(repository)).ok()).max_by_key(|entry|version_key(&entry.version)))
}
pub fn newer(latest: &str, current: &str) -> bool {
    let current=version_key(current).or_else(||{let(core,dev)=current.split_once("-dev.")?;if dev.parse::<u32>().ok()?==0{return None;}let mut key=version_key(core)?;key.1=0;Some(key)});
    matches!((version_key(latest),current),(Some(a),Some(b)) if a>b)
}
fn client() -> Result<reqwest::blocking::Client,UpdateError> {
    reqwest::blocking::Client::builder().connect_timeout(std::time::Duration::from_secs(15)).timeout(std::time::Duration::from_secs(3600)).user_agent("SantaCita").https_only(true).build().map_err(|_|UpdateError::Network)
}
fn response_bytes(response: reqwest::blocking::Response, limit:u64) -> Result<Vec<u8>,UpdateError> {
    let mut raw=Vec::new();response.take(limit+1).read_to_end(&mut raw).map_err(|_|UpdateError::Network)?;
    if raw.len() as u64>limit { return Err(UpdateError::Invalid); } Ok(raw)
}
fn get(client:&reqwest::blocking::Client,url:&str,limit:u64) -> Result<Vec<u8>,UpdateError> {
    let response=client.get(url).send().and_then(reqwest::blocking::Response::error_for_status).map_err(|_|UpdateError::Network)?;
    response_bytes(response,limit)
}
fn find_release(client:&reqwest::blocking::Client,include_beta:bool) -> Result<Option<(Latest,String)>,UpdateError> {
    for repository in REPOSITORIES {
        let mut best:Option<(Latest,String)>=None;
        for page in 1..=10 {
            let suffix=if include_beta { format!("/releases?per_page=100&page={page}") } else { "/releases/latest".into() };
            let response=client.get(api_url(repository,&suffix)?).header("Accept","application/vnd.github+json").timeout(std::time::Duration::from_secs(15)).send().map_err(|_|UpdateError::Network)?;
            if response.status()==reqwest::StatusCode::NOT_FOUND && page==1 {break;}
            let raw=response_bytes(response.error_for_status().map_err(|_|UpdateError::Network)?,8*1024*1024)?;
            let body=String::from_utf8(raw).map_err(|_|UpdateError::Invalid)?;
            let entries:Value=serde_json::from_str(&body).map_err(|_|UpdateError::Invalid)?;
            if !include_beta { return Ok(Some((parse_entry(&entries,false,Some(repository))?,body))); }
            let list=entries.as_array().ok_or(UpdateError::Invalid)?;
            for entry in list {
                if let Ok(candidate)=parse_entry(entry,true,Some(repository)) {
                    if best.as_ref().is_none_or(|(old,_)|version_key(&candidate.version)>version_key(&old.version)) {best=Some((candidate,entry.to_string()));}
                }
            }
            if list.len()<100 {break;}
            if page==10 {return Err(UpdateError::Invalid);}
        }
        if best.is_some(){return Ok(best);}
    }
    Ok(None)
}
pub fn fetch_latest_for(include_beta:bool) -> Result<Option<Latest>,UpdateError> {
    Ok(find_release(&client()?,include_beta)?.map(|(latest,_)|latest))
}
pub fn fetch_latest() -> Result<Latest,UpdateError> { fetch_latest_for(false)?.ok_or(UpdateError::Invalid) }

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
    pub attestation_url: String,
    pub repository: String,
}
pub const MAX_PACKAGE: u64 = 2 * 1024 * 1024 * 1024;
fn package_names(version: &str, target: Target) -> Vec<String> {
    match (target.system, target.arch) {
        ("macos", arch) => {
            let chip = if arch == "arm64" { "Apple-Silicon" } else { "Intel" };
            vec![format!("SantaCita-{version}-macOS-{chip}.dmg")]
        }
        ("windows", arch) => vec![format!("SantaCita-{version}-windows-{arch}.msi")],
        _ => vec![format!("SantaCita-{version}-x86_64.AppImage")],
    }
}
/// The package for `target` when the latest release is newer than `current`; `None` when it is not.
/// The release must be immutable, carry a `SHA256SUMS` file, and every file must be the project's own
/// GitHub release asset.
pub fn select_release(body: &str, current: &str, target: Target) -> Result<Option<Release>, UpdateError> {
    select_channel_release(body,current,target,false,false)
}
/// The portable ZIP (Windows only) instead of the installer, for a portable install.
pub fn select_portable_release(body: &str, current: &str, target: Target) -> Result<Option<Release>, UpdateError> {
    if target.system != "windows" {
        return Err(UpdateError::Invalid);
    }
    select_channel_release(body,current,target,true,false)
}
/// Whether this program is a portable install: Windows, with a plain `portable.flag` beside the program.
pub fn is_portable_install() -> bool {
    cfg!(windows) && std::env::current_exe().ok().and_then(|exe| exe.parent().map(crate::portable_update::portable_flag_in)).unwrap_or(false)
}
pub fn select_channel_release(body:&str,current:&str,target:Target,portable:bool,include_beta:bool)->Result<Option<Release>,UpdateError>{
    if portable && target.system!="windows" {return Err(UpdateError::Invalid);}
    if !matches!((target.system,target.arch),("macos","arm64"|"x64")|("windows"|"linux","x64")){return Err(UpdateError::Invalid);}
    let raw:Value=serde_json::from_str(body).map_err(|_|UpdateError::Invalid)?;
    let latest=parse_entry(&raw,include_beta,None)?;
    let names=if portable {vec![format!("SantaCita-{}-windows-{}-portable.zip",latest.version,target.arch)]} else {package_names(&latest.version,target)};
    if !newer(&latest.version, current) {
        return Ok(None);
    }
    let raw: serde_json::Value = serde_json::from_str(body).map_err(|_| UpdateError::Invalid)?;
    if raw["immutable"] != true {
        return Err(UpdateError::Invalid);
    }
    let base = format!("{}/releases/download/v{}/", latest.repository, latest.version);
    let assets = raw["assets"].as_array().ok_or(UpdateError::Invalid)?;
    let asset = |name: &str| -> Option<(String, u64)> {
        let mut found = assets.iter().filter(|a| a["name"] == name);
        let first = found.next()?;
        let url = first["browser_download_url"].as_str()?;
        (found.next().is_none() && url == format!("{base}{name}")).then(|| (url.to_owned(), first["size"].as_u64().unwrap_or(0)))
    };
    let (name, (url, size)) = names.into_iter()
        .find_map(|name| asset(&name).map(|found| (name, found)))
        .ok_or(UpdateError::Invalid)?;
    let (sums_url, _) = asset("SHA256SUMS").ok_or(UpdateError::Invalid)?;
    if size == 0 || size > MAX_PACKAGE {
        return Err(UpdateError::Invalid);
    }
    let (attestation_url, _) = asset(&format!("{name}.sigstore.json")).ok_or(UpdateError::Invalid)?;
    Ok(Some(Release { version: latest.version, name, url, sums_url, size, attestation_url, repository:latest.repository }))
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
/// Download and verify a stable release (compatibility wrapper).
pub fn download_latest(current:&str,dir:&std::path::Path,cancel:&std::sync::atomic::AtomicBool,progress:impl FnMut(u8))->Result<std::path::PathBuf,UpdateError>{
    download_channel(current,dir,cancel,progress,false)
}
pub fn download_channel(current:&str,dir:&std::path::Path,cancel:&std::sync::atomic::AtomicBool,progress:impl FnMut(u8),include_beta:bool)->Result<std::path::PathBuf,UpdateError>{
    use std::sync::atomic::Ordering;
    if cancel.load(Ordering::Acquire){return Err(UpdateError::Cancelled);}
    let client=client()?;
    let (_,body)=find_release(&client,include_beta)?.ok_or(UpdateError::Invalid)?;
    let release=select_channel_release(&body,current,Target::current().ok_or(UpdateError::Invalid)?,is_portable_install(),include_beta)?.ok_or(UpdateError::Invalid)?;
    let sums=get(&client,&release.sums_url,16*1024)?;
    let digest=checksum_for(std::str::from_utf8(&sums).map_err(|_|UpdateError::Invalid)?,&release.name)?;
    let response=client.get(&release.url).send().and_then(reqwest::blocking::Response::error_for_status).map_err(|_|UpdateError::Network)?;
    let path=stream_verified(response,release.size,&digest,&release.name,dir,cancel,progress)?;
    let result=(||{
        let commit=release_commit(&client,&release.repository,&release.version)?;
        let proof=get(&client,&release.attestation_url,crate::attestations::MAX_BUNDLE)?;
        crate::attestations::verify(&proof,&release.name,&digest,&release.version,&release.repository,&commit)?;
        if cancel.load(Ordering::Acquire){return Err(UpdateError::Cancelled);}
        let receipt=serde_json::json!({"version":release.version,"repository":release.repository,"commit":commit,"bundle":serde_json::from_slice::<Value>(&proof).map_err(|_|UpdateError::Invalid)?});
        std::fs::write(path.with_file_name(format!("{}.provenance.json",release.name)),receipt.to_string()).map_err(|_|UpdateError::Invalid)?;
        Ok(path.clone())
    })();
    if result.is_err(){let _=std::fs::remove_file(&path);let _=std::fs::remove_file(path.with_file_name(format!("{}.sha256",release.name)));}
    result
}
fn release_commit(client:&reqwest::blocking::Client,repository:&str,version:&str)->Result<String,UpdateError>{
    let mut url=api_url(repository,&format!("/git/ref/tags/v{version}"))?;
    for _ in 0..4{
        let raw=get(client,&url,16384)?;let value:Value=serde_json::from_slice(&raw).map_err(|_|UpdateError::Invalid)?;
        let object=&value["object"];let sha=object["sha"].as_str().ok_or(UpdateError::Invalid)?;
        if sha.len()!=40||!sha.bytes().all(|b|b.is_ascii_hexdigit()){return Err(UpdateError::Invalid);}
        match object["type"].as_str(){Some("commit")=>return Ok(sha.into()),Some("tag")=>url=api_url(repository,&format!("/git/tags/{sha}"))?,_=>return Err(UpdateError::Invalid)}
    }
    Err(UpdateError::Invalid)
}
pub fn verify_download(path:&std::path::Path,include_beta:bool)->Result<(),UpdateError>{
    use sha2::{Digest,Sha256};
    let name=path.file_name().and_then(|v|v.to_str()).ok_or(UpdateError::Invalid)?;
    let read=|suffix:&str,limit:u64|->Result<Vec<u8>,UpdateError>{let file=std::fs::File::open(path.with_file_name(format!("{name}{suffix}"))).map_err(|_|UpdateError::Invalid)?;let mut data=Vec::new();file.take(limit+1).read_to_end(&mut data).map_err(|_|UpdateError::Invalid)?;if data.len()as u64>limit{return Err(UpdateError::Invalid);}Ok(data)};
    let sums=read(".sha256",16384)?;let digest=checksum_for(std::str::from_utf8(&sums).map_err(|_|UpdateError::Invalid)?,name)?;
    let receipt=read(".provenance.json",crate::attestations::MAX_BUNDLE+4096)?;
    let value:Value=serde_json::from_slice(&receipt).map_err(|_|UpdateError::Invalid)?;
    let version=value["version"].as_str().ok_or(UpdateError::Invalid)?;
    if version_key(version).ok_or(UpdateError::Invalid)?.1==1&&!include_beta{return Err(UpdateError::Invalid);}
    let mut source=std::fs::File::open(path).map_err(|_|UpdateError::Invalid)?;let mut hash=Sha256::new();std::io::copy(&mut source,&mut hash).map_err(|_|UpdateError::Invalid)?;
    if format!("{:x}",hash.finalize())!=digest{return Err(UpdateError::Invalid);}
    crate::attestations::verify(value["bundle"].to_string().as_bytes(),name,&digest,version,value["repository"].as_str().ok_or(UpdateError::Invalid)?,value["commit"].as_str().ok_or(UpdateError::Invalid)?)
}
