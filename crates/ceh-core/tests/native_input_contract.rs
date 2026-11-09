use ceh_core::input::{selection_keys,SelectionKey,POINTER_TIMING};
#[test]
fn selection_requires_backend_verified_commit_without_form_submission(){
 assert_eq!(selection_keys(0,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Commit]);
 assert_eq!(selection_keys(2,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Down,SelectionKey::Down,SelectionKey::Commit]);
 assert!(selection_keys(257,true).is_err());assert!(selection_keys(2,false).is_err());
 assert!(POINTER_TIMING.move_ms>0);assert!(POINTER_TIMING.press_ms>0);assert!(POINTER_TIMING.move_ms+POINTER_TIMING.press_ms<1000);
}
