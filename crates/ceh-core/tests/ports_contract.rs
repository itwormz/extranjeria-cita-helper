use ceh_core::ports::*;

#[test]
fn manual_clock_separates_wall_adjustments_from_monotonic_time() {
    let mut clock = ManualClock::new(ClockReading {
        unix_millis: 1000,
        monotonic_millis: 100,
    });
    clock.advance(500).unwrap();
    assert_eq!(
        clock.now(),
        ClockReading {
            unix_millis: 1500,
            monotonic_millis: 600
        }
    );
    clock.set_wall(-1000);
    assert_eq!(
        clock.now(),
        ClockReading {
            unix_millis: -1000,
            monotonic_millis: 600
        }
    );
}

#[test]
fn clock_overflow_is_an_error_without_partial_advance() {
    let initial = ClockReading {
        unix_millis: i64::MAX,
        monotonic_millis: u64::MAX,
    };
    let mut clock = ManualClock::new(initial);
    assert_eq!(clock.advance(1), Err(PortError::InvalidData));
    assert_eq!(clock.now(), initial);
}

#[test]
fn secret_material_is_not_debug_output() {
    let secret = SecretBytes::new(b"fixture-secret".to_vec());
    assert!(!format!("{secret:?}").contains("fixture-secret"));
    assert_eq!(secret.expose(), b"fixture-secret");
}
