use ceh_core::input::{selection_keys,SelectionKey,POINTER_TIMING};
#[test]
fn selection_requires_backend_verified_commit_without_form_submission(){
 assert_eq!(selection_keys(0,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Commit]);
 assert_eq!(selection_keys(2,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Down,SelectionKey::Down,SelectionKey::Commit]);
 assert!(selection_keys(257,true).is_err());assert!(selection_keys(2,false).is_err());
 assert!(POINTER_TIMING.move_ms>0);assert!(POINTER_TIMING.press_ms>0);assert!(POINTER_TIMING.move_ms+POINTER_TIMING.press_ms<1000);
}

use ceh_core::input::{pointer_path, Reach};
const OPEN: Reach = Reach { left: 2000, right: 2000, up: 2000, down: 2000 };
#[test]
fn the_cursor_travels_a_curved_path_that_ends_on_the_target() {
    for seed in [1u64, 7, 99, 123_456_789] {
        let path = pointer_path(seed, OPEN);
        assert!(path.len() >= 8, "a path of several moves, got {}", path.len());
        let last = path.last().unwrap();
        assert_eq!((last.x, last.y), (0, 0), "ends exactly on the element centre");
        let first = path.first().unwrap();
        assert!(first.x.abs() + first.y.abs() >= 100, "starts away from the target");
        let total: u64 = path.iter().map(|s| s.ms).sum();
        assert!((250..=700).contains(&total), "total move time {total} ms");
        // Not a straight line: some point is off the chord from start to target.
        let off_chord = path.iter().any(|s| {
            let cross = (first.x as i64 * s.y as i64 - first.y as i64 * s.x as i64).abs();
            cross > 400
        });
        assert!(off_chord, "the path bends (seed {seed})");
        // It keeps getting closer overall: distance at the end is below the start and the middle.
        let dist = |s: &ceh_core::input::PointerStep| ((s.x as f64).powi(2) + (s.y as f64).powi(2)).sqrt();
        assert!(dist(path.last().unwrap()) < dist(&path[path.len() / 2]));
    }
    assert_ne!(pointer_path(1, OPEN), pointer_path(2, OPEN), "different seeds give different paths");
    assert_eq!(pointer_path(5, OPEN), pointer_path(5, OPEN), "a seed gives the same path");
}

#[test]
fn the_cursor_never_leaves_the_room_the_element_has_in_the_viewport() {
    // An element near the left edge, 200 px from it, 40 px from the top.
    let reach = Reach { left: 198, right: 900, up: 38, down: 600 };
    for seed in 0..200u64 {
        for step in pointer_path(seed, reach) {
            assert!(step.x >= -198 && step.x <= 900 && step.y >= -38 && step.y <= 600, "seed {seed}: {step:?}");
        }
        assert_eq!((pointer_path(seed, reach).last().unwrap().x, pointer_path(seed, reach).last().unwrap().y), (0, 0));
    }
    // No room at all (unknown geometry): the path collapses onto the target instead of leaving the page.
    let none = Reach { left: 0, right: 0, up: 0, down: 0 };
    assert!(pointer_path(3, none).iter().all(|s| (s.x, s.y) == (0, 0)));
}
