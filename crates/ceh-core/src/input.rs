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

/// One move of the cursor: a position relative to the target element's centre and the time the move takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerStep {
    pub x: i32,
    pub y: i32,
    pub ms: u64,
}
fn next(state: &mut u64) -> f64 {
    // xorshift64*: small, dependency-free, deterministic per seed.
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
}
/// A curved cursor path that ends exactly on the element centre (a cubic Bezier from a point 180–420 px away,
/// eased in and out, with a little jitter), about 300–600 ms in total. The browser's own pointer input
/// performs it, so every move reaches the page as a trusted `mousemove`.
pub fn pointer_path(seed: u64) -> Vec<PointerStep> {
    const STEPS: usize = 12;
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for _ in 0..4 {
        next(&mut state);
    }
    let distance = 180.0 + next(&mut state) * 240.0;
    let angle = next(&mut state) * std::f64::consts::TAU;
    let (sx, sy) = (angle.cos() * distance, angle.sin() * distance);
    // Perpendicular bulge of the two control points, on opposite sides of the chord for an S-curve half the time.
    let (px, py) = (-sy / distance, sx / distance);
    let bulge1 = (next(&mut state) - 0.5) * distance * 0.6;
    let bulge2 = (next(&mut state) - 0.5) * distance * 0.4;
    let c1 = (sx * 0.66 + px * bulge1, sy * 0.66 + py * bulge1);
    let c2 = (sx * 0.28 + px * bulge2, sy * 0.28 + py * bulge2);
    let total = 260.0 + distance * 0.7 + next(&mut state) * 60.0;
    let mut path = Vec::with_capacity(STEPS);
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let eased = t * t * (3.0 - 2.0 * t);
        let u = 1.0 - eased;
        let mut x = u.powi(3) * sx + 3.0 * u * u * eased * c1.0 + 3.0 * u * eased * eased * c2.0;
        let mut y = u.powi(3) * sy + 3.0 * u * u * eased * c1.1 + 3.0 * u * eased * eased * c2.1;
        if i < STEPS {
            x += (next(&mut state) - 0.5) * 3.0;
            y += (next(&mut state) - 0.5) * 3.0;
        } else {
            (x, y) = (0.0, 0.0);
        }
        // The easing already spaces the positions (dense at both ends, wide in the middle), so equal step times
        // give the slow-fast-slow speed.
        let ms = (total / STEPS as f64).round() as u64;
        path.push(PointerStep { x: x.round() as i32, y: y.round() as i32, ms });
    }
    path
}
