//! Own native window only. Never changes system appearance or user preferences.
use tauri::{WebviewUrl, WebviewWindowBuilder};
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("CEH — проверка темы заголовка")
            .inner_size(640.0, 180.0)
            .on_navigation(|url| url.as_str() == "about:blank")
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(), &'static str> {
                    for dark in [true, false, true] {
                        ceh_desktop::window_theme::apply(&window, dark)?;
                        let actual = ceh_desktop::window_theme::is_dark(&window)?;
                        println!("requested_dark={dark} actual_dark={actual}");
                        if actual != dark {
                            return Err("native_theme_mismatch");
                        }
                        window
                            .set_title(if dark {
                                "CEH — тёмный заголовок"
                            } else {
                                "CEH — светлый заголовок"
                            })
                            .map_err(|_| "window_title")?;
                        std::thread::sleep(std::time::Duration::from_secs(4));
                    }
                    Ok(())
                })();
                println!("native_theme_result={result:?}");
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("theme probe startup");
}
