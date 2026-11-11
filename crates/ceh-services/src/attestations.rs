//! Offline Sigstore verification with the same release policy as Python 0.3.8.
use crate::{release_versions::version_key, updates::{REPOSITORIES,UpdateError}};
use serde_json::{Value,json};
use sigstore_verify::{Verifier,VerificationPolicy,types::{Bundle,SignatureContent,Sha256Hash},trust_root::{TrustedRoot,SIGSTORE_PRODUCTION_TRUSTED_ROOT},crypto::FulcioCiClaims};
pub const MAX_BUNDLE:u64=2*1024*1024;
pub const WORKFLOW:&str=".github/workflows/build.yml";
fn valid_hex(value:&str,len:usize)->bool{value.len()==len&&value.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}

pub fn verify(raw:&[u8],name:&str,digest:&str,version:&str,repository:&str,commit:&str)->Result<(),UpdateError>{
    if raw.len()as u64>MAX_BUNDLE||!REPOSITORIES.contains(&repository)||version_key(version).is_none()||!valid_hex(digest,64)||!valid_hex(commit,40){return Err(UpdateError::Invalid);}
    // Reject duplicate keys before any parser can choose a different meaning.
    unique_json(raw)?;
    let bundle=Bundle::from_json(std::str::from_utf8(raw).map_err(|_|UpdateError::Invalid)?).map_err(|_|UpdateError::Invalid)?;
    let root=TrustedRoot::from_json(SIGSTORE_PRODUCTION_TRUSTED_ROOT).map_err(|_|UpdateError::Invalid)?;
    let verifier=Verifier::new(&root).map_err(|_|UpdateError::Invalid)?;
    let identity=format!("{repository}/{WORKFLOW}@refs/tags/v{version}");
    let result=verifier.verify(Sha256Hash::from_hex(digest).map_err(|_|UpdateError::Invalid)?,&bundle,&VerificationPolicy::new(identity,"https://token.actions.githubusercontent.com")).map_err(|_|UpdateError::Invalid)?;
    let claims=&result.certificate().ok_or(UpdateError::Invalid)?.ci_claims;
    let SignatureContent::DsseEnvelope(envelope)=&bundle.content else{return Err(UpdateError::Invalid)};
    if envelope.payload_type!="application/vnd.in-toto+json"{return Err(UpdateError::Invalid);}
    let statement=unique_json(envelope.payload.as_bytes())?;
    validate_policy(&statement,claims,name,digest,version,repository,commit)
}
fn validate_policy(statement:&Value,claims:&FulcioCiClaims,name:&str,digest:&str,version:&str,repository:&str,commit:&str)->Result<(),UpdateError>{
    let reference=format!("refs/tags/v{version}");
    if claims.source_repository_uri.as_deref()!=Some(repository)||claims.source_repository_ref.as_deref()!=Some(&reference)||claims.source_repository_digest.as_deref()!=Some(commit)||claims.runner_environment.as_deref()!=Some("github-hosted")||claims.build_trigger.as_deref()!=Some("push"){return Err(UpdateError::Invalid);}
    if statement["_type"]!="https://in-toto.io/Statement/v1"||statement["predicateType"]!="https://slsa.dev/provenance/v1"{return Err(UpdateError::Invalid);}
    let subjects=statement["subject"].as_array().ok_or(UpdateError::Invalid)?;
    let selected:Vec<_>=subjects.iter().filter(|s|s["name"]==name).collect();
    if selected.len()!=1||selected[0]["digest"]!=json!({"sha256":digest}){return Err(UpdateError::Invalid);}
    let definition=&statement["predicate"]["buildDefinition"];
    if definition["buildType"]!="https://actions.github.io/buildtypes/workflow/v1"||definition["externalParameters"]["workflow"]!=json!({"repository":repository,"path":WORKFLOW,"ref":reference})||definition["internalParameters"]["github"]["event_name"]!="push"||definition["internalParameters"]["github"]["runner_environment"]!="github-hosted"{return Err(UpdateError::Invalid);}
    let sources=definition["resolvedDependencies"].as_array().ok_or(UpdateError::Invalid)?;
    let uri=format!("git+{repository}@{reference}");let found:Vec<_>=sources.iter().filter(|s|s["uri"]==uri).collect();
    if found.len()!=1||found[0]["digest"]["gitCommit"]!=commit{return Err(UpdateError::Invalid);}
    Ok(())
}
// A serde visitor is used only for duplicate rejection; normal types still validate the bundle.
fn unique_json(raw:&[u8])->Result<Value,UpdateError>{
    use serde::{Deserialize,Deserializer,de::{self,Visitor,MapAccess,SeqAccess}};
    struct Unique(Value);
    impl<'de> Deserialize<'de> for Unique{
        fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
            struct V;
            impl<'de> Visitor<'de> for V{
                type Value=Unique;
                fn expecting(&self,f:&mut std::fmt::Formatter)->std::fmt::Result{f.write_str("unambiguous JSON")}
                fn visit_map<M:MapAccess<'de>>(self,mut m:M)->Result<Unique,M::Error>{let mut map=serde_json::Map::new();while let Some((k,v))=m.next_entry::<String,Unique>()?{if map.insert(k,v.0).is_some(){return Err(de::Error::custom("duplicate JSON key"));}}Ok(Unique(Value::Object(map)))}
                fn visit_seq<S:SeqAccess<'de>>(self,mut s:S)->Result<Unique,S::Error>{let mut list=Vec::new();while let Some(v)=s.next_element::<Unique>()?{list.push(v.0);}Ok(Unique(Value::Array(list)))}
                fn visit_str<E:de::Error>(self,v:&str)->Result<Unique,E>{Ok(Unique(json!(v)))}
                fn visit_bool<E:de::Error>(self,v:bool)->Result<Unique,E>{Ok(Unique(json!(v)))}
                fn visit_u64<E:de::Error>(self,v:u64)->Result<Unique,E>{Ok(Unique(json!(v)))}
                fn visit_i64<E:de::Error>(self,v:i64)->Result<Unique,E>{Ok(Unique(json!(v)))}
                fn visit_f64<E:de::Error>(self,v:f64)->Result<Unique,E>{Ok(Unique(json!(v)))}
                fn visit_unit<E:de::Error>(self)->Result<Unique,E>{Ok(Unique(Value::Null))}
            }
            d.deserialize_any(V)
        }
    }
    serde_json::from_slice::<Unique>(raw).map(|v|v.0).map_err(|_|UpdateError::Invalid)
}
pub fn verify_file(path:&std::path::Path,version:&str,repository:&str,commit:&str)->Result<(),UpdateError>{
    use sha2::{Digest,Sha256};use std::io::Read;
    let name=path.file_name().and_then(|n|n.to_str()).ok_or(UpdateError::Invalid)?;
    let mut file=std::fs::File::open(path).map_err(|_|UpdateError::Invalid)?;let mut hash=Sha256::new();std::io::copy(&mut file,&mut hash).map_err(|_|UpdateError::Invalid)?;
    let mut proof=Vec::new();std::fs::File::open(path.with_file_name(format!("{name}.sigstore.json"))).map_err(|_|UpdateError::Invalid)?.take(MAX_BUNDLE+1).read_to_end(&mut proof).map_err(|_|UpdateError::Invalid)?;
    verify(&proof,name,&format!("{:x}",hash.finalize()),version,repository,commit)
}
#[cfg(test)]
mod tests{
    use super::*;
    const RAW:&[u8]=include_bytes!("../../../contracts/fixtures/public-ceh-0.3.7.sigstore.json");
    const NAME:&str="extranjeria-cita-helper-0.3.7-macos-arm64.dmg";
    const DIGEST:&str="46ec5571f9b946ab6a059d322592266b5ef29b674f6cdf82ae771abe6979aa0d";
    const COMMIT:&str="8208a873d7a496454550090f620eae7660891083";
    #[test]fn actual_published_bundle_verifies_offline(){assert_eq!(verify(RAW,NAME,DIGEST,"0.3.7",REPOSITORIES[1],COMMIT),Ok(()));}
    #[test]fn signer_and_commit_cannot_be_substituted(){
        assert!(verify(RAW,NAME,DIGEST,"0.3.7",REPOSITORIES[0],COMMIT).is_err());
        assert!(verify(RAW,NAME,DIGEST,"0.3.7",REPOSITORIES[1],&"a".repeat(40)).is_err());
        assert!(verify(RAW,"other.dmg",DIGEST,"0.3.7",REPOSITORIES[1],COMMIT).is_err());
        assert!(verify(RAW,NAME,&"a".repeat(64),"0.3.7",REPOSITORIES[1],COMMIT).is_err());
        assert!(verify(br#"{"x":1,"x":2}"#,NAME,DIGEST,"0.3.7",REPOSITORIES[1],COMMIT).is_err());
    }
}
