//! Capture policy is checked before rendering and again before persisting an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureState { Unlocked, Locked, Unavailable }
impl CaptureState {
    pub fn reason(self) -> &'static str {
        match self { Self::Unlocked => "capture_failed", Self::Locked => "console_locked", Self::Unavailable => "lock_state_unavailable" }
    }
}
pub fn capture_if_allowed<T>(mut state: impl FnMut() -> CaptureState, capture: impl FnOnce() -> Option<T>) -> Result<T, &'static str> {
    let before=state();
    if before != CaptureState::Unlocked { return Err(before.reason()); }
    let image=capture().ok_or("capture_failed")?;
    let after=state();
    if after != CaptureState::Unlocked { return Err(after.reason()); }
    Ok(image)
}
