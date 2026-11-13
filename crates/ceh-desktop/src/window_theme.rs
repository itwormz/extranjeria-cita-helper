//! Theme only for this application/window; never changes the macOS system theme.
#[cfg(feature = "shell")]
pub fn apply(window: &tauri::WebviewWindow, dark: bool) -> Result<(), &'static str> {
    window
        .set_theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .map_err(|_| "theme_unavailable")?;
    // An explicitly painted native background also avoids a light first frame.
    let color = if dark {
        tauri::window::Color(29, 30, 34, 255)
    } else {
        tauri::window::Color(255, 255, 255, 255)
    };
    window
        .set_background_color(Some(color))
        .map_err(|_| "theme_unavailable")?;
    #[cfg(target_os = "macos")]
    window
        .set_title_bar_style(tauri::TitleBarStyle::Transparent)
        .map_err(|_| "theme_unavailable")?;
    #[cfg(target_os = "macos")]
    on_main(window, move |window| {
        use objc2_app_kit::{
            NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua,
            NSAppearanceNameDarkAqua, NSWindow,
        };
        // Tauri's macOS theme setter updates NSApplication. Explicit NSWindow
        // appearance also keeps native title-bar chrome aligned with the web content.
        let name = unsafe {
            if dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        let appearance = NSAppearance::appearanceNamed(name).ok_or("theme_unavailable")?;
        let raw = window
            .ns_window()
            .map_err(|_| "theme_unavailable")?
            .cast::<NSWindow>();
        // Obtained from this live Tauri window; used only on its main thread.
        let native = unsafe { raw.as_ref() }.ok_or("theme_unavailable")?;
        native.setAppearance(Some(&appearance));
        Ok(())
    })?;
    Ok(())
}
/// Native readback for acceptance checks; not exposed through UI IPC.
#[cfg(feature = "shell")]
pub fn is_dark(window: &tauri::WebviewWindow) -> Result<bool, &'static str> {
    #[cfg(not(target_os = "macos"))]
    return window
        .theme()
        .map(|v| v == tauri::Theme::Dark)
        .map_err(|_| "theme_unavailable");
    #[cfg(target_os = "macos")]
    on_main(window, |window| {
        use objc2_app_kit::{NSAppearanceCustomization, NSWindow};
        let raw = window
            .ns_window()
            .map_err(|_| "theme_unavailable")?
            .cast::<NSWindow>();
        let native = unsafe { raw.as_ref() }.ok_or("theme_unavailable")?;
        Ok(native
            .effectiveAppearance()
            .name()
            .to_string()
            .contains("Dark"))
    })
}
#[cfg(all(feature = "shell", target_os = "macos"))]
fn on_main<T: Send + 'static>(
    window: &tauri::WebviewWindow,
    action: impl FnOnce(tauri::WebviewWindow) -> Result<T, &'static str> + Send + 'static,
) -> Result<T, &'static str> {
    let owned = window.clone();
    if objc2::MainThreadMarker::new().is_some() {
        return action(owned);
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let _ = sender.send(action(owned));
        })
        .map_err(|_| "theme_unavailable")?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| "theme_unavailable")?
}

/// Only the supported application at the PID retained by the search controller may be raised.
pub fn supported_macos_browser(bundle: &str) -> bool {
    matches!(bundle, "org.mozilla.firefox" | "com.google.Chrome" | "com.brave.Browser" | "org.chromium.Chromium")
}
/// Activate the exact owned browser process, never the user's default browser.
#[cfg(feature = "shell")]
pub fn show_owned_browser(window:&tauri::WebviewWindow,pid:u32)->Result<(),&'static str>{
 #[cfg(target_os="macos")]
 return on_main(window,move |_|{
  use objc2_app_kit::{NSApplication,NSRunningApplication,NSApplicationActivationOptions};
  let pid=i32::try_from(pid).map_err(|_|"browser_not_open")?;
  let app=NSRunningApplication::runningApplicationWithProcessIdentifier(pid).ok_or("browser_not_open")?;
  if app.bundleIdentifier().is_none_or(|id| !supported_macos_browser(&id.to_string())){return Err("browser_not_open");}
  let current=NSRunningApplication::currentApplication();
  let main=objc2::MainThreadMarker::new().ok_or("browser_show_failed")?;
  NSApplication::sharedApplication(main).yieldActivationToApplication(&app);
  let _=app.unhide();
  if !app.activateFromApplication_options(&current,NSApplicationActivationOptions::ActivateAllWindows){return Err("browser_show_failed");}
  Ok(())
 });
 #[cfg(not(target_os="macos"))]{let _=window;crate::window_activation::show(pid)}
}
