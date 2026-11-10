//! Every sampled timing of the Python assist settings has a consumer in production code; a timing that is
//! parsed and saved but never used silently changes the pace of the search.
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}
#[test]
fn every_timing_kind_is_sampled_by_production_code() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for name in ["ceh-browser", "ceh-core", "ceh-desktop"] {
        sources(&crates.join(name).join("src"), &mut files);
    }
    files.retain(|f| f.file_name().is_some_and(|n| n != "timings.rs"));
    let code: String = files.iter().map(|f| std::fs::read_to_string(f).unwrap()).collect();
    for kind in ["Bootstrap", "Action", "Typing", "ContactTyping", "Reload"] {
        assert!(code.contains(&format!("TimingKind::{kind}")), "TimingKind::{kind} is never sampled");
    }
}
