use serde_json::json;
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    root
}
#[test]
fn exports_resolve_local_record_and_never_include_booking_secrets_in_public_model() {
    let root = root();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-31T15:00:00Z")
        .unwrap()
        .to_utc();
    let row=ceh_services::exports::AchievementStore::new(&root.path().join("data/achievements.json")).record("fixture",&json!({"checks":48}),&json!({"date":"2026-11-12","time":"10:15","office":"PRIVATE-OFFICE","address":"PRIVATE-ADDRESS","confirmation":"PRIVATE-CONFIRMATION","timezone":"Europe/Madrid"}),at).unwrap();
    let id = row["id"].as_str().unwrap();
    let model =
        ceh_desktop::achievement_exports::public_model(root.path(), "fixture", id, "ru").unwrap();
    let text = model.to_string();
    for private in [
        "PRIVATE-OFFICE",
        "PRIVATE-ADDRESS",
        "PRIVATE-CONFIRMATION",
        "2026-11-12",
        "10:15",
    ] {
        assert!(!text.contains(private));
    }
    assert!(model["post"].as_str().unwrap().contains("48"));
    assert_eq!(model["layouts"]["dark"]["width"], 1200);
    assert_eq!(model["layouts"]["dark"]["height"], 1240);
    assert_ne!(model["layouts"]["dark"]["background"], model["layouts"]["light"]["background"]);
    assert!(model.get("card").is_none());
    let calendar =
        ceh_desktop::achievement_exports::calendar_model(root.path(), "fixture", id).unwrap();
    assert!(
        calendar["google_url"]
            .as_str()
            .unwrap()
            .contains("calendar.google.com")
    );
    assert!(!calendar.to_string().contains("PRIVATE-CONFIRMATION"));
    assert!(
        ceh_desktop::achievement_exports::public_model(root.path(), "foreign", id, "ru").is_err()
    );
    assert!(
        ceh_desktop::achievement_exports::public_model(root.path(), "fixture", "foreign", "ru")
            .is_err()
    );
    assert!(
        ceh_desktop::achievement_exports::public_model(root.path(), "fixture", id, "unexpected")
            .is_err()
    );
}
#[test]
fn png_validator_accepts_only_bounded_achievement_card_dimensions() {
    assert!(ceh_desktop::achievement_exports::decode_card(b"not png").is_err());
    assert!(ceh_desktop::achievement_exports::decode_card(&vec![0; 8 * 1024 * 1024 + 1]).is_err());
    for (w, h, ok) in [(1200, 1240, true), (1200, 900, false), (12, 9, false)] {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![0; w as usize * h as usize * 4])
                .unwrap();
        }
        let result = ceh_desktop::achievement_exports::decode_card(&bytes);
        assert_eq!(result.is_ok(), ok);
        if let Ok(rgba) = result {
            assert_eq!(rgba.len(), 1200 * 1240 * 4);
        }
    }
}

#[test]
fn reddit_is_an_accepted_share_network_and_unknown_networks_are_not() {
    use ceh_desktop::achievement_exports::ExportAction;
    for network in ["telegram", "x", "facebook", "instagram", "reddit"] {
        let action: Result<ExportAction, _> = serde_json::from_value(serde_json::json!({"action":"prepare_text","network":network,"text":"hello"}));
        assert!(action.is_ok(), "{network}");
    }
    let unknown: Result<ExportAction, _> = serde_json::from_value(serde_json::json!({"action":"prepare_text","network":"mastodon","text":"hello"}));
    assert!(unknown.is_err());
}

#[test]
fn every_supported_browser_has_a_public_download_page() {
    use ceh_desktop::achievement_exports::{PublicLink, public_link};
    for (link, host) in [
        (PublicLink::Firefox, "mozilla.org"),
        (PublicLink::Chrome, "google.com"),
        (PublicLink::Brave, "brave.com"),
        (PublicLink::Edge, "microsoft.com"),
        (PublicLink::Chromium, "chromium.org"),
    ] {
        let url = public_link(link, "ru").unwrap();
        assert!(url.starts_with("https://") && url.contains(host), "{url}");
    }
}
