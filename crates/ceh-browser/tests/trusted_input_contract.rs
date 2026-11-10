//! Design principle 1 (.local browser-layer design §3): page scripts only read; every change to the page
//! is made by the browser's own input (`input.performActions`). A script may be listed in `ALLOWED` only
//! together with the design-document section that records the exception.
use std::path::Path;

const FORBIDDEN: &[(&str, &str)] = &[
    (r"dispatchEvent", "synthetic event"),
    (r"\.click\s*\(", "script click"),
    (r"\.value\s*=[^=]", "direct value write"),
    (r"\.checked\s*=[^=]", "direct checked write"),
    (r"\.selectedIndex\s*=[^=]", "direct selection write"),
    (r"\.selected\s*=[^=]", "direct option write"),
    (r"setter\s*\.\s*call", "native value setter"),
    (r"\.set\s*\.\s*call", "native value setter"),
    (r"\.focus\s*\(", "script focus"),
    (r"\.select\s*\(", "script selection"),
    (r"\.submit\s*\(|requestSubmit", "script submit"),
    (r"execCommand", "script text insertion"),
    (r"\.scrollTo\s*\(|\.scrollBy\s*\(", "script scroll"),
];
/// (file, pattern, design section). Empty: no exceptions are recorded.
const ALLOWED: &[(&str, &str, &str)] = &[];

fn matches(pattern: &str, text: &str) -> bool {
    regex::Regex::new(pattern).unwrap().is_match(text)
}

fn scripts() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "js"))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap()))
        .collect();
    found.sort();
    found
}

#[test]
fn matcher_recognises_the_forbidden_forms_and_ignores_comparisons() {
    assert!(matches(r"\.value\s*=[^=]", "target.value = 'x'"));
    assert!(matches(r"\.value\s*=[^=]", "target.value='x'"));
    assert!(!matches(r"\.value\s*=[^=]", "target.value==='x'"));
    assert!(!matches(r"\.value\s*=[^=]", "if (select.value === other) return"));
    assert!(matches(r"\.click\s*\(", "radio.click();"));
    assert!(matches(r"\.submit\s*\(|requestSubmit", "form.requestSubmit()"));
    assert!(!matches(r"dispatchEvent", "target.dispatch()"));
}

#[test]
fn step_scripts_never_mutate_the_page_directly() {
    let scripts = scripts();
    assert!(scripts.len() >= 15, "script list is incomplete: {}", scripts.len());
    let mut violations = Vec::new();
    for (name, code) in &scripts {
        for (pattern, what) in FORBIDDEN {
            if matches(pattern, code) && !ALLOWED.iter().any(|(file, p, _)| file == name && p == pattern) {
                violations.push(format!("{name}: {what}"));
            }
        }
    }
    assert!(violations.is_empty(), "scripts that change the page themselves:\n{}", violations.join("\n"));
}
