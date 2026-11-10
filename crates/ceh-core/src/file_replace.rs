//! Atomic replace that tolerates a reader briefly holding the target (Python 0.3.7 `file_replace.replace_file`).
//! Windows refuses the rename with "access denied" while another process, for example the window reading
//! the same JSON, has the target open. Readers close quickly, so a short bounded backoff is enough.
use std::{io, time::Duration};

pub const DELAYS_MS: [u64; 6] = [10, 20, 50, 100, 200, 300];

/// Run `attempt` until it succeeds. `attempt` gets the state back after a failure (a temporary file that
/// could not be moved yet), so the caller never loses it; only `PermissionDenied` is retried.
pub fn replace_with<S>(
    mut state: S,
    mut attempt: impl FnMut(S) -> Result<(), (S, io::Error)>,
    mut sleep: impl FnMut(Duration),
) -> io::Result<()> {
    for delay in DELAYS_MS {
        match attempt(state) {
            Ok(()) => return Ok(()),
            Err((back, error)) if error.kind() == io::ErrorKind::PermissionDenied => {
                state = back;
                sleep(Duration::from_millis(delay));
            }
            Err((_, error)) => return Err(error),
        }
    }
    attempt(state).map_err(|(_, error)| error)
}

/// [`replace_with`] with real sleeping.
pub fn replace<S>(state: S, attempt: impl FnMut(S) -> Result<(), (S, io::Error)>) -> io::Result<()> {
    replace_with(state, attempt, std::thread::sleep)
}
