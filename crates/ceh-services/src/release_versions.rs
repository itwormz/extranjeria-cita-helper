//! Canonical stable/beta release ordering, shared by updater and provenance policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionKey(pub [u32; 3], pub u8, pub u32);
pub fn version_key(value: &str) -> Option<VersionKey> {
    let (core, beta) = match value.split_once("-beta.") { Some((core,n)) => (core,Some(n)), None => (value,None) };
    let number = |s: &str| -> Option<u32> { (!s.is_empty() && s.bytes().all(|b|b.is_ascii_digit()) && (s=="0" || !s.starts_with('0'))).then(||s.parse().ok()).flatten() };
    let parts: Vec<_> = core.split('.').map(number).collect();
    if parts.len()!=3 { return None; }
    let core=[parts[0]?,parts[1]?,parts[2]?];
    match beta { Some(n) => { let n=number(n)?; (n>0).then_some(VersionKey(core,1,n)) }, None => Some(VersionKey(core,2,0)) }
}
