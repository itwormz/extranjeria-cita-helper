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
