use ceh_services::updates::{newer, parse_release_list, select_channel_release, Target};

#[test]
fn beta_numbers_and_stable_are_ordered_without_losing_prerelease_information(){
 assert!(newer("0.4.0-beta.10","0.4.0-beta.2"));
 assert!(newer("0.4.0","0.4.0-beta.10"));
 assert!(!newer("0.4.0-beta.10","0.4.0"));
 assert!(!newer("0.4.0-rc.1","0.3.8"));
}
#[test]
fn list_selection_is_opt_in_and_uses_the_current_repository(){
 let repo="https://github.com/itwormz/extranjeria-cita-helper";
 let entries=serde_json::json!([
 {"tag_name":"v0.4.0-beta.2","prerelease":true,"html_url":format!("{repo}/releases/tag/v0.4.0-beta.2")},
 {"tag_name":"v0.3.8","prerelease":false,"html_url":format!("{repo}/releases/tag/v0.3.8")},
 {"tag_name":"v0.4.0-beta.10","prerelease":true,"html_url":format!("{repo}/releases/tag/v0.4.0-beta.10")}
 ]).to_string();
 assert_eq!(parse_release_list(&entries,false,repo).unwrap().unwrap().version,"0.3.8");
 assert_eq!(parse_release_list(&entries,true,repo).unwrap().unwrap().version,"0.4.0-beta.10");
}
#[test]
fn beta_package_needs_its_own_source_proof_and_checksum(){
 let repo="https://github.com/itwormz/extranjeria-cita-helper";
 let name="SantaCita-0.4.0-beta.1-windows-x64.msi";
 let assets:Vec<_>=[name,"SHA256SUMS",&format!("{name}.sigstore.json")].into_iter().map(|n|serde_json::json!({"name":n,"size":7,"browser_download_url":format!("{repo}/releases/download/v0.4.0-beta.1/{n}")})).collect();
 let mut raw=serde_json::json!({"tag_name":"v0.4.0-beta.1","prerelease":true,"immutable":true,"html_url":format!("{repo}/releases/tag/v0.4.0-beta.1"),"assets":assets});
 let target=Target{system:"windows",arch:"x64"};
 let found=select_channel_release(&raw.to_string(),"0.3.8",target,false,true).unwrap().unwrap();
 assert_eq!(found.repository,repo);
 assert_eq!(found.attestation_url,format!("{repo}/releases/download/v0.4.0-beta.1/{name}.sigstore.json"));
 assert!(select_channel_release(&raw.to_string(),"0.3.8",target,false,false).is_err());
 raw["assets"][2]["browser_download_url"]=serde_json::json!("https://github.com/itwormz/santacita/other");
 assert!(select_channel_release(&raw.to_string(),"0.3.8",target,false,true).is_err());
}

fn published_release(version: &str, prerelease: bool, name: &str) -> serde_json::Value {
    let repo = "https://github.com/itwormz/extranjeria-cita-helper";
    let proof = format!("{name}.sigstore.json");
    let assets: Vec<_> = [name, "SHA256SUMS", &proof]
        .into_iter()
        .map(|asset| serde_json::json!({
            "name": asset, "size": 7,
            "browser_download_url": format!("{repo}/releases/download/v{version}/{asset}")
        }))
        .collect();
    serde_json::json!({
        "tag_name": format!("v{version}"), "prerelease": prerelease, "draft": false,
        "immutable": true, "html_url": format!("{repo}/releases/tag/v{version}"), "assets": assets
    })
}

#[test]
fn beta_channel_selects_exact_packages_for_every_published_target() {
    let version = "0.4.0-beta.10";
    for (system, arch, portable, suffix) in [
        ("macos", "arm64", false, "macOS-Apple-Silicon.dmg"),
        ("macos", "x64", false, "macOS-Intel.dmg"),
        ("windows", "x64", false, "windows-x64.msi"),
        ("windows", "x64", true, "windows-x64-portable.zip"),
        ("linux", "x64", false, "x86_64.AppImage"),
    ] {
        let name = format!("SantaCita-{version}-{suffix}");
        let body = published_release(version, true, &name).to_string();
        let target = Target { system, arch };
        assert_eq!(
            select_channel_release(&body, "0.3.8", target, portable, true)
                .unwrap().unwrap().name,
            name
        );
        assert!(select_channel_release(&body, "0.3.8", target, portable, false).is_err());
        assert!(select_channel_release(&body, "0.4.0", target, portable, true).unwrap().is_none());
    }
}

#[test]
fn opted_in_channel_prefers_final_release_and_ignores_drafts_or_mismatched_flags() {
    let repo = "https://github.com/itwormz/extranjeria-cita-helper";
    let mut draft = published_release("9.0.0-beta.1", true, "unused");
    draft["draft"] = true.into();
    let entries = serde_json::json!([
        published_release("0.4.0", false, "unused"),
        published_release("0.4.0-beta.10", true, "unused"),
        published_release("0.5.0-beta.1", false, "unused"),
        published_release("0.6.0", true, "unused"),
        draft
    ]).to_string();
    for include_beta in [false, true] {
        assert_eq!(
            parse_release_list(&entries, include_beta, repo).unwrap().unwrap().version,
            "0.4.0"
        );
    }
}

#[test]
fn beta_release_preserves_immutable_and_unique_asset_requirements() {
    let name = "SantaCita-0.4.0-beta.1-windows-x64.msi";
    let target = Target { system: "windows", arch: "x64" };
    let release = published_release("0.4.0-beta.1", true, name);
    for change in ["mutable", "missing_checksum", "missing_proof", "duplicate", "foreign_package"] {
        let mut raw = release.clone();
        match change {
            "mutable" => raw["immutable"] = false.into(),
            "missing_checksum" => { raw["assets"].as_array_mut().unwrap().remove(1); }
            "missing_proof" => { raw["assets"].as_array_mut().unwrap().remove(2); }
            "duplicate" => {
                let duplicate = raw["assets"][0].clone();
                raw["assets"].as_array_mut().unwrap().push(duplicate);
            }
            "foreign_package" => raw["assets"][0]["browser_download_url"] =
                format!("https://example.invalid/{name}").into(),
            _ => unreachable!(),
        }
        assert!(select_channel_release(&raw.to_string(), "0.3.8", target, false, true).is_err(), "{change}");
    }
}
