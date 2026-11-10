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
const ALLOWED: &[(&str, &str, &str)] = &[
    // Python parity (`site._select_without_focus`): a `<select>` is chosen by ONE input + change event from the
    // page script, without focus. Choosing it with keyboard type-ahead produced a burst of selection changes
    // and requests that the site's WAF answered with a block (live timeline 2026-11-10 21:48); the script
    // choice passed it in every Rust run before and in Python. See .local/plans/santacita/f5-block-findings.md.
    ("selection-step.js", r"dispatchEvent", "select choice, Python parity"),
    ("selection-step.js", r"\.value\s*=[^=]", "select choice, Python parity"),
    ("applicant-step.js", r"dispatchEvent", "select choice, Python parity"),
    ("applicant-step.js", r"\.value\s*=[^=]", "select choice, Python parity"),
];

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

#[test]
fn a_select_is_chosen_exactly_like_python_with_one_input_and_one_change_event() {
    for name in ["selection-step.js", "applicant-step.js"] {
        let code = scripts().into_iter().find(|(n, _)| n == name).unwrap().1;
        assert_eq!(code.matches("new Event('input', {bubbles: true, composed: true})").count(), 1, "{name}");
        assert_eq!(code.matches("new Event('change', {bubbles: true})").count(), 1, "{name}");
        assert_eq!(code.matches("dispatchEvent").count(), 2, "{name}: only the select choice dispatches events");
        assert!(code.contains("select.value = option.value") || code.contains("select.value = options[0].value"), "{name}");
    }
    // The type-ahead path (a click on the select, then a key per character) is gone.
    let typed = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/typed_select.rs")).unwrap();
    assert!(!typed.contains("trusted_input::key") && !typed.contains("type_text") && !typed.contains("trusted_input::focus"));
}
