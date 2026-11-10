use ceh_core::file_replace::{replace_with, DELAYS_MS};
use std::{cell::RefCell, io, time::Duration};

fn denied() -> io::Error {
    io::Error::from(io::ErrorKind::PermissionDenied)
}

#[test]
fn a_target_briefly_held_by_a_reader_is_replaced_after_bounded_backoff() {
    let slept = RefCell::new(Vec::new());
    let mut failures = 3;
    let result = replace_with("temp", |state| if failures > 0 { failures -= 1; Err((state, denied())) } else { Ok(()) }, |d| slept.borrow_mut().push(d));
    assert!(result.is_ok());
    assert_eq!(*slept.borrow(), vec![Duration::from_millis(10), Duration::from_millis(20), Duration::from_millis(50)]);
}

#[test]
fn the_state_is_handed_back_to_every_attempt_and_the_last_error_surfaces_after_the_delays_run_out() {
    let slept = RefCell::new(0);
    let mut attempts = 0;
    let result = replace_with(0u32, |state| { attempts += 1; Err((state + 1, denied())) }, |_| *slept.borrow_mut() += 1);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(attempts, DELAYS_MS.len() + 1, "one try per delay plus the final one");
    assert_eq!(*slept.borrow(), DELAYS_MS.len());
}

#[test]
fn other_errors_are_not_retried() {
    let slept = RefCell::new(0);
    let mut attempts = 0;
    let result = replace_with((), |state| { attempts += 1; Err((state, io::Error::from(io::ErrorKind::NotFound))) }, |_| *slept.borrow_mut() += 1);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_eq!((attempts, *slept.borrow()), (1, 0));
}
