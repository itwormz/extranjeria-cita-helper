use ceh_services::updates::{newer, parse_latest, UpdateError};

fn release(tag: &str, extra: &str) -> String {
    format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/extranjeria-cita-helper/releases/tag/{tag}"{extra}}}"#)
}

#[test]
fn a_higher_stable_release_is_offered_with_its_public_page() {
    let latest = parse_latest(&release("v1.2.0", "")).unwrap();
    assert_eq!(latest.version, "1.2.0");
    assert_eq!(latest.url, "https://github.com/itwormz/extranjeria-cita-helper/releases/tag/v1.2.0");
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
        r#"{"tag_name":"v1.2.0","draft":true,"prerelease":false,"html_url":"https://github.com/itwormz/extranjeria-cita-helper/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":true,"html_url":"https://github.com/itwormz/extranjeria-cita-helper/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"1.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/extranjeria-cita-helper/releases/tag/1.2.0"}"#,
        r#"{"tag_name":"v1.2","draft":false,"prerelease":false,"html_url":"https://github.com/itwormz/extranjeria-cita-helper/releases/tag/v1.2"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":false,"html_url":"https://evil.example/releases/tag/v1.2.0"}"#,
        r#"{"tag_name":"v1.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/other/repo/releases/tag/v1.2.0"}"#,
        "not json",
    ] {
        assert_eq!(parse_latest(bad).err(), Some(UpdateError::Invalid), "{bad}");
    }
}
