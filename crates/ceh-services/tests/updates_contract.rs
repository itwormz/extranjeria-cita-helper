use ceh_services::updates::{newer, parse_latest, UpdateError};

fn release(tag: &str, extra: &str) -> String {
    format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/santacita/releases/tag/{tag}"{extra}}}"#)
}

#[test]
fn a_higher_stable_release_is_offered_with_its_public_page() {
    let latest = parse_latest(&release("v1.2.0", "")).unwrap();
    assert_eq!(latest.version, "1.2.0");
    assert_eq!(latest.url, "https://github.com/itwormz/santacita/releases/tag/v1.2.0");
    assert!(newer("1.2.0", "1.1.9"));
    assert!(newer("1.10.0", "1.9.0"), "numbers, not text");
    assert!(!newer("1.2.0", "1.2.0"));
    assert!(!newer("1.1.0", "1.2.0"));
}

#[test]
fn a_development_build_is_older_than_every_release_of_its_number() {
    assert!(newer("1.0.0", "0.0.0-dev.1"));
    assert!(newer("1.0.0", "1.0.0-dev.3"), "the release of the same number replaces its dev build");
    assert!(!newer("1.0.0-dev.3", "1.0.0"));
}

#[test]
fn drafts_prereleases_odd_tags_and_foreign_pages_are_refused() {
    for bad in [
        r#"{"tag_name":"v1.2.0","draft":true,"prerelease":false,"html_url":"https://github.com/itwormz/santacita/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":true,"html_url":"https://github.com/itwormz/santacita/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"1.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/santacita/releases/tag/1.2.0"}"#,
        r#"{"tag_name":"v1.2","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/santacita/releases/tag/v1.2"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":false,"html_url":"https://evil.example/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/other/repo/releases/tag/v1.2.0"}"#,
        "not json",
    ] {
        assert_eq!(parse_latest(bad).err(), Some(UpdateError::Invalid), "{bad}");
    }
}

mod package {
    use ceh_services::updates::{checksum_for, select_release, stream_verified, Target, UpdateError};
    use sha2::{Digest, Sha256};
    use std::sync::atomic::AtomicBool;

    fn api(version: &str, assets: &[(&str, u64)], immutable: bool) -> String {
        let mut list: Vec<String> = assets
            .iter()
            .map(|(name, size)| format!(r#"{{"name":"{name}","size":{size},"browser_download_url":"https://github.com/itwormz/santacita/releases/download/v{version}/{name}"}}"#))
            .collect();
        for (name,_) in assets.iter().filter(|(name,_)|*name!="SHA256SUMS") {
            list.push(format!(r#"{{"name":"{name}.sigstore.json","size":1000,"browser_download_url":"https://github.com/itwormz/santacita/releases/download/v{version}/{name}.sigstore.json"}}"#));
        }
        format!(r#"{{"tag_name":"v{version}","draft":false,"prerelease":false,"immutable":{immutable},"html_url":"https://github.com/itwormz/santacita/releases/tag/v{version}","assets":[{}]}}"#, list.join(","))
    }
    const MAC_ARM: Target = Target { system: "macos", arch: "arm64" };

    #[test]
    fn the_package_of_this_system_and_its_checksum_file_are_selected_from_the_release() {
        let body = api("1.2.0", &[("SantaCita-1.2.0-macOS-Apple-Silicon.dmg", 1000), ("SantaCita-1.2.0-windows-x64.msi", 1), ("SHA256SUMS", 300)], true);
        let release = select_release(&body, "1.0.0", MAC_ARM).unwrap().unwrap();
        assert_eq!(release.name, "SantaCita-1.2.0-macOS-Apple-Silicon.dmg");
        assert_eq!(release.size, 1000);
        assert!(release.url.ends_with("/v1.2.0/SantaCita-1.2.0-macOS-Apple-Silicon.dmg"));
        assert!(release.sums_url.ends_with("/v1.2.0/SHA256SUMS"));
        let windows = select_release(&body, "1.0.0", Target { system: "windows", arch: "x64" }).unwrap().unwrap();
        assert_eq!(windows.name, "SantaCita-1.2.0-windows-x64.msi");
        let missing = select_release(&body, "1.0.0", Target { system: "linux", arch: "x64" });
        assert_eq!(missing.err(), Some(UpdateError::Invalid), "no package for this system");
    }

    #[test]
    fn nothing_is_offered_when_current_or_when_the_release_is_not_trustworthy() {
        let body = api("1.2.0", &[("SantaCita-1.2.0-macOS-Apple-Silicon.dmg", 1000), ("SHA256SUMS", 300)], true);
        assert_eq!(select_release(&body, "1.2.0", MAC_ARM).unwrap(), None);
        let mutable = api("1.2.0", &[("SantaCita-1.2.0-macOS-Apple-Silicon.dmg", 1000), ("SHA256SUMS", 300)], false);
        assert_eq!(select_release(&mutable, "1.0.0", MAC_ARM).err(), Some(UpdateError::Invalid), "a release that can still change");
        let no_sums = api("1.2.0", &[("SantaCita-1.2.0-macOS-Apple-Silicon.dmg", 1000)], true);
        assert_eq!(select_release(&no_sums, "1.0.0", MAC_ARM).err(), Some(UpdateError::Invalid));
        let huge = api("1.2.0", &[("SantaCita-1.2.0-macOS-Apple-Silicon.dmg", 3 * 1024 * 1024 * 1024), ("SHA256SUMS", 300)], true);
        assert_eq!(select_release(&huge, "1.0.0", MAC_ARM).err(), Some(UpdateError::Invalid));
        let foreign = body.replace("https://github.com/itwormz/santacita/releases/download", "https://evil.example/download");
        assert_eq!(select_release(&foreign, "1.0.0", MAC_ARM).err(), Some(UpdateError::Invalid), "assets must come from the project's releases");
    }

    #[test]
    fn a_checksum_line_must_name_the_package_exactly_once() {
        let digest = "a".repeat(64);
        let file = format!("{digest}  a.dmg\n{}  b.dmg\n", "b".repeat(64));
        assert_eq!(checksum_for(&file, "a.dmg").unwrap(), digest);
        assert!(checksum_for(&file, "c.dmg").is_err());
        assert!(checksum_for(&format!("{file}{digest}  a.dmg\n"), "a.dmg").is_err(), "ambiguous");
    }

    #[test]
    fn a_download_is_kept_only_when_its_size_and_checksum_match_and_progress_is_reported() {
        let data = vec![7u8; 3 * 1024 * 1024 + 11];
        let digest = format!("{:x}", Sha256::digest(&data));
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();
        let path = stream_verified(&data[..], data.len() as u64, &digest, "p.dmg", dir.path(), &AtomicBool::new(false), |p| seen.push(p)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(seen.last(), Some(&100));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
        assert!(std::fs::read_to_string(dir.path().join("p.dmg.sha256")).unwrap().starts_with(&digest));
        // Wrong checksum: nothing is left behind.
        let other = tempfile::tempdir().unwrap();
        assert!(stream_verified(&data[..], data.len() as u64, &"0".repeat(64), "p.dmg", other.path(), &AtomicBool::new(false), |_| {}).is_err());
        assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 0);
        // More bytes than announced, and a cancel.
        let third = tempfile::tempdir().unwrap();
        assert!(stream_verified(&data[..], 10, &digest, "p.dmg", third.path(), &AtomicBool::new(false), |_| {}).is_err());
        assert!(stream_verified(&data[..], data.len() as u64, &digest, "p.dmg", third.path(), &AtomicBool::new(true), |_| {}).is_err());
        assert_eq!(std::fs::read_dir(third.path()).unwrap().count(), 0);
    }
}

mod portable {
    use ceh_services::updates::{select_portable_release, Target, UpdateError};
    const WINDOWS: Target = Target { system: "windows", arch: "x64" };
    fn api(assets: &[&str]) -> String {
        let mut list: Vec<String> = assets
            .iter()
            .map(|name| format!(r#"{{"name":"{name}","size":1000,"browser_download_url":"https://github.com/itwormz/santacita/releases/download/v1.2.0/{name}"}}"#))
            .collect();
        for name in assets.iter().filter(|name|**name!="SHA256SUMS") {
            list.push(format!(r#"{{"name":"{name}.sigstore.json","size":1000,"browser_download_url":"https://github.com/itwormz/santacita/releases/download/v1.2.0/{name}.sigstore.json"}}"#));
        }
        format!(r#"{{"tag_name":"v1.2.0","draft":false,"prerelease":false,"immutable":true,"html_url":"https://github.com/itwormz/santacita/releases/tag/v1.2.0","assets":[{}]}}"#, list.join(","))
    }

    #[test]
    fn a_portable_install_is_offered_the_portable_zip_never_the_installer() {
        let body = api(&["SantaCita-1.2.0-windows-x64.msi", "SantaCita-1.2.0-windows-x64-portable.zip", "SHA256SUMS"]);
        let release = select_portable_release(&body, "1.0.0", WINDOWS).unwrap().unwrap();
        assert_eq!(release.name, "SantaCita-1.2.0-windows-x64-portable.zip");
        assert!(release.url.ends_with("/v1.2.0/SantaCita-1.2.0-windows-x64-portable.zip"));
        assert_eq!(select_portable_release(&body, "1.2.0", WINDOWS).unwrap(), None);
    }

    #[test]
    fn without_a_portable_package_or_off_windows_nothing_is_guessed() {
        let only_msi = api(&["SantaCita-1.2.0-windows-x64.msi", "SHA256SUMS"]);
        assert_eq!(select_portable_release(&only_msi, "1.0.0", WINDOWS).err(), Some(UpdateError::Invalid));
        let body = api(&["SantaCita-1.2.0-windows-x64-portable.zip", "SHA256SUMS"]);
        assert_eq!(select_portable_release(&body, "1.0.0", Target { system: "macos", arch: "arm64" }).err(), Some(UpdateError::Invalid));
    }
}
