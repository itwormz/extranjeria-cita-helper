use ceh_desktop::presentation::{
    AttentionTimer, IconStatus, StatusFacts, icon_status, status_pixels,
};
#[test]
fn status_priority_matches_existing_desktop_handoff_pause_and_failure_rules() {
    let cases = [
        (serde_json::json!({}), false, false, IconStatus::Idle),
        (serde_json::json!({}), true, false, IconStatus::Running),
        (
            serde_json::json!({"mode":"finished"}),
            true,
            false,
            IconStatus::Idle,
        ),
        (
            serde_json::json!({"paused":true}),
            true,
            false,
            IconStatus::Paused,
        ),
        (
            serde_json::json!({"paused":true}),
            false,
            false,
            IconStatus::Idle,
        ),
        (
            serde_json::json!({"mode":"manual"}),
            false,
            false,
            IconStatus::Attention,
        ),
        (
            serde_json::json!({"booking_handoff":true,"paused":true}),
            true,
            false,
            IconStatus::Attention,
        ),
        (
            serde_json::json!({"browser_retained":true}),
            false,
            false,
            IconStatus::Attention,
        ),
        (
            serde_json::json!({"failed":true,"mode":"manual"}),
            true,
            false,
            IconStatus::Error,
        ),
        (
            serde_json::json!({"connection_lost":true}),
            true,
            false,
            IconStatus::Error,
        ),
        (serde_json::json!({}), false, true, IconStatus::Error),
    ];
    for (raw, active, error, expected) in cases {
        let facts: StatusFacts = serde_json::from_value(raw).unwrap();
        assert_eq!(icon_status(&facts, active, error), expected);
    }
}
#[test]
fn status_indicator_preserves_base_icon_and_uses_original_status_palette() {
    let base = vec![0; 64 * 64 * 4];
    assert_eq!(
        status_pixels(&base, 64, 64, IconStatus::Idle).unwrap(),
        base
    );
    for (status, rgb) in [
        (IconStatus::Running, [55, 187, 120]),
        (IconStatus::Paused, [139, 142, 153]),
        (IconStatus::Attention, [237, 181, 72]),
        (IconStatus::Error, [229, 101, 104]),
    ] {
        let result = status_pixels(&base, 64, 64, status).unwrap();
        let center = (54 * 64 + 54) * 4;
        assert_eq!(&result[center..center + 3], &rgb);
        assert_eq!(&result[..32 * 64 * 4], &base[..32 * 64 * 4]);
        assert_eq!(base, vec![0; 64 * 64 * 4]);
    }
    assert!(status_pixels(&[], 64, 64, IconStatus::Running).is_err());
    assert!(status_pixels(&[0; 4], 0, 1, IconStatus::Idle).is_err());
}
#[test]
fn attention_timer_never_catches_up_or_restarts_after_stop() {
    let mut timer = AttentionTimer::new(3000);
    timer.start(1000, true);
    assert!(timer.due(1000));
    assert!(!timer.due(3999));
    assert!(timer.due(4000));
    assert!(timer.due(20000));
    assert!(!timer.due(20001));
    timer.stop();
    assert!(!timer.due(99999));
    timer.start(30000, false);
    assert!(timer.due(30000));
    assert!(!timer.due(99999));
}

#[test]
fn a_large_icon_is_shrunk_by_averaging_so_small_window_icons_stay_sharp() {
    use ceh_desktop::presentation::shrink_icon;
    // 4x4 -> 2x2: each output pixel is the average of a 2x2 block, weighted by alpha.
    let mut base = vec![0u8; 4 * 4 * 4];
    for (i, pixel) in base.chunks_mut(4).enumerate() {
        let (x, y) = (i % 4, i / 4);
        let colour = if (x / 2 + y / 2) % 2 == 0 { [200, 0, 0, 255] } else { [0, 0, 200, 255] };
        pixel.copy_from_slice(&colour);
    }
    let (out, w, h) = shrink_icon(&base, 4, 4, 2).unwrap();
    assert_eq!((w, h, out.len()), (2, 2, 16));
    assert_eq!(&out[0..4], &[200, 0, 0, 255]);
    assert_eq!(&out[4..8], &[0, 0, 200, 255]);
    // A half-transparent mix keeps the colour of the opaque pixels, not a darkened blend.
    let mut mixed = vec![0u8; 2 * 2 * 4];
    mixed[0..4].copy_from_slice(&[100, 100, 100, 255]);
    let (out, _, _) = shrink_icon(&mixed, 2, 2, 2).unwrap();
    assert_eq!(&out[0..3], &[100, 100, 100]);
    assert_eq!(out[3], 64);
    // Sizes that do not divide evenly, a zero factor and a short buffer are refused.
    assert!(shrink_icon(&base, 4, 4, 3).is_err() && shrink_icon(&base, 4, 4, 0).is_err() && shrink_icon(&base[..10], 4, 4, 2).is_err());
}
