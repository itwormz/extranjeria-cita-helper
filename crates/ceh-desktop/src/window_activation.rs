//! Native activation only for the process owned by this search. No shell or browser URLs.
use std::sync::{Mutex, mpsc};
static ACTIVATING: Mutex<()> = Mutex::new(());

pub(crate) fn show(pid: u32) -> Result<(), &'static str> {
    if pid == 0 { return Err("browser_not_open"); }
    let (sender, receiver)=mpsc::sync_channel(1);
    std::thread::spawn(move || {
        // A stalled display server cannot accumulate activation workers on repeated clicks.
        let result=match ACTIVATING.try_lock() {
            Ok(_guard) => activate(pid),
            Err(_) => Err("browser_show_failed"),
        };
        let _=sender.send(result);
    });
    receiver.recv_timeout(std::time::Duration::from_secs(3)).map_err(|_| "browser_show_failed")?
}

#[cfg(target_os = "windows")]
fn activate(pid: u32) -> Result<(), &'static str> {
    use std::ffi::c_void;
    type Hwnd = *mut c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, data: isize) -> i32;
        fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(window: Hwnd) -> i32;
        fn IsIconic(window: Hwnd) -> i32;
        fn ShowWindow(window: Hwnd, command: i32) -> i32;
        fn SetForegroundWindow(window: Hwnd) -> i32;
        fn GetForegroundWindow() -> Hwnd;
    }
    struct Found { pid: u32, window: Hwnd }
    unsafe extern "system" fn collect(window: Hwnd, data: isize) -> i32 {
        let found=unsafe { &mut *(data as *mut Found) };
        let mut owner=0;
        unsafe { GetWindowThreadProcessId(window, &mut owner); }
        if owner==found.pid && unsafe { IsWindowVisible(window) } != 0 {
            found.window=window;
            return 0;
        }
        1
    }
    let mut found=Found {pid, window: std::ptr::null_mut()};
    unsafe { EnumWindows(collect, &mut found as *mut Found as isize); }
    if found.window.is_null() { return Err("browser_not_open"); }
    // Recheck ownership after enumeration, and do not unmaximize a regular window.
    let mut owner=0;
    unsafe {
        GetWindowThreadProcessId(found.window, &mut owner);
        if owner != pid { return Err("browser_not_open"); }
        if IsIconic(found.window) != 0 { ShowWindow(found.window, 9); }
        if SetForegroundWindow(found.window) != 0 || GetForegroundWindow()==found.window { Ok(()) }
        else { Err("browser_show_failed") }
    }
}

#[cfg(target_os = "linux")]
fn activate(pid: u32) -> Result<(), &'static str> {
    use x11rb::{connection::Connection, protocol::xproto::{ConnectionExt, AtomEnum, ClientMessageEvent, EventMask, CLIENT_MESSAGE_EVENT}};
    // Native Wayland does not expose other applications' windows. XWayland works when the browser
    // exports an X11 window; otherwise report unavailable instead of activating a different app.
    let display=std::env::var("DISPLAY").map_err(|_| "browser_show_unavailable")?;
    if !(display.starts_with(':') || display.starts_with("unix:")) { return Err("browser_show_unavailable"); }
    let (connection, screen)=x11rb::connect(Some(&display)).map_err(|_| "browser_show_unavailable")?;
    let root=connection.setup().roots.get(screen).ok_or("browser_show_unavailable")?.root;
    let atom=|name: &[u8]| connection.intern_atom(false, name).map_err(|_| "browser_show_failed")?.reply().map(|v| v.atom).map_err(|_| "browser_show_failed");
    let clients=atom(b"_NET_CLIENT_LIST")?;
    let owner=atom(b"_NET_WM_PID")?;
    let active=atom(b"_NET_ACTIVE_WINDOW")?;
    let windows=connection.get_property(false, root, clients, AtomEnum::WINDOW, 0, 4096)
        .map_err(|_| "browser_show_failed")?.reply().map_err(|_| "browser_show_failed")?;
    let mut found=None;
    for window in windows.value32().ok_or("browser_show_unavailable")?.take(4096) {
        let Ok(cookie)=connection.get_property(false, window, owner, AtomEnum::CARDINAL, 0, 1) else {continue};
        let Ok(property)=cookie.reply() else {continue};
        if property.value32().and_then(|mut values| values.next())==Some(pid) {found=Some(window);break;}
    }
    let window=found.ok_or("browser_show_unavailable")?;
    connection.map_window(window).map_err(|_| "browser_show_failed")?.check().map_err(|_| "browser_show_failed")?;
    // EWMH source 2: explicit desktop-controller action, never a forced focus of an unrelated window.
    let event=ClientMessageEvent::new(32, window, active, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
    debug_assert_eq!(event.response_type, CLIENT_MESSAGE_EVENT);
    connection.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event)
        .map_err(|_| "browser_show_failed")?.check().map_err(|_| "browser_show_failed")?;
    connection.flush().map_err(|_| "browser_show_failed")?;
    let until=std::time::Instant::now()+std::time::Duration::from_millis(750);
    while std::time::Instant::now()<until {
        let property=connection.get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
            .map_err(|_| "browser_show_failed")?.reply().map_err(|_| "browser_show_failed")?;
        if property.value32().and_then(|mut values| values.next())==Some(window) {return Ok(());}
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Err("browser_show_failed")
}
