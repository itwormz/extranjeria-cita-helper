//! Shared interaction semantics. No backend-specific humanization or UI handles.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SelectionKey{Home,Down,Commit}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct InputLimit;
pub fn selection_keys(enabled_index:usize,safe_commit:bool)->Result<Vec<SelectionKey>,InputLimit>{
 if !safe_commit||enabled_index>256{return Err(InputLimit);}
 let mut keys=Vec::with_capacity(enabled_index+2);keys.push(SelectionKey::Home);keys.extend(std::iter::repeat_n(SelectionKey::Down,enabled_index));keys.push(SelectionKey::Commit);Ok(keys)
}
pub struct PointerTiming{pub move_ms:u64,pub press_ms:u64}
pub const POINTER_TIMING:PointerTiming=PointerTiming{move_ms:120,press_ms:60};
