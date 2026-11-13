use ceh_desktop::{diagnostic_capture::{CaptureState, capture_if_allowed}, window_theme::supported_macos_browser};
use std::cell::Cell;

#[test]
fn owned_supported_browser_identity_includes_each_macos_engine() {
    for id in ["org.mozilla.firefox", "com.google.Chrome", "com.brave.Browser", "org.chromium.Chromium"] {
        assert!(supported_macos_browser(id), "{id}");
    }
    for id in ["com.apple.Safari", "com.google.Chrome.fake", "", "org.mozilla.firefox.extra"] {
        assert!(!supported_macos_browser(id));
    }
}

#[test]
fn locked_or_unknown_console_never_invokes_capture() {
    for state in [CaptureState::Locked, CaptureState::Unavailable] {
        let calls = Cell::new(0);
        let result = capture_if_allowed(|| state, || {calls.set(calls.get()+1); Some(vec![1])});
        assert_eq!(calls.get(), 0);
        assert_eq!(result.unwrap_err(), state.reason());
    }
}

#[test]
fn console_is_checked_again_before_a_capture_can_be_saved() {
    let states = Cell::new(0);
    let calls = Cell::new(0);
    let result = capture_if_allowed(|| {
        let n=states.get(); states.set(n+1);
        if n==0 {CaptureState::Unlocked} else {CaptureState::Locked}
    }, || {calls.set(calls.get()+1); Some(vec![1])});
    assert_eq!(calls.get(), 1);
    assert_eq!(result.unwrap_err(), "console_locked");
    assert_eq!(capture_if_allowed(|| CaptureState::Unlocked, || Some(vec![1])).unwrap(), vec![1]);
}
