//! Opt-in native presentation acceptance. No profiles, browser or network.
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PageFacts},
    model::*,
};
use ceh_desktop::{session_presentation::*, sound::SoundPlayback};
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder};
fn page(kind: PageKind, name: &str) -> Observation {
    Observation {
        document: DocumentId::new(name).unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn main() {
    let sound = std::env::args().any(|a| a == "--sound");
    tauri::Builder::default()
        .setup(move |app| {
            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("CEH — проверка нативных статусов, без поиска")
            .inner_size(640.0, 180.0)
            .on_navigation(|url| url.as_str() == "about:blank")
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    let parent = std::path::Path::new(".local/builds/work/native-presentation");
                    std::fs::create_dir_all(parent).map_err(|_| "probe directory")?;
                    let root = tempfile::tempdir_in(parent).map_err(|_| "probe root")?;
                    let session =
                        SessionId::new("native-presentation-probe").map_err(|_| "session")?;
                    let mut flow = FlowEngine::new(session.clone(), SearchOptions::default());
                    let mut presenter = SessionPresentation::new(session, 10000);
                    let mut playback = SoundPlayback::default();
                    let mut seq = 0;
                    let mut show = |label: &str,
                                    control: &ceh_core::control::ControlSnapshot,
                                    frame: PresentationFrame,
                                    effects: &[FlowEffect]|
                     -> Result<(), String> {
                        seq += 1;
                        let now = seq * 4000;
                        let change = presenter
                            .update(control, seq, now, frame, effects)
                            .map_err(|_| "presentation update")?;
                        window
                            .set_title(&format!("CEH — проверка: {label} (без поиска)"))
                            .map_err(|_| "probe title")?;
                        apply_native(&window, root.path(), &mut playback, change)
                            .map_err(str::to_owned)?;
                        println!("{label}: {:?} {:?}", change.icon, change.sound);
                        std::thread::sleep(Duration::from_secs(4));
                        Ok(())
                    };
                    let frame = PresentationFrame {
                        active: true,
                        sound_enabled: sound,
                        ..Default::default()
                    };
                    show("работа", &flow.control(), frame, &[])?;
                    let effects = flow
                        .observe_with_facts(
                            page(PageKind::Office, "office"),
                            PageFacts {
                                office_count: 2,
                                ..Default::default()
                            },
                            0,
                        )
                        .map_err(|_| "office")?;
                    show("выбор офиса — внимание", &flow.control(), frame, &effects)?;
                    let control = flow.pause(1).map_err(|_| "pause")?;
                    show("пауза — тишина", &control, frame, &[])?;
                    let control = flow.resume(2).map_err(|_| "resume")?;
                    show("продолжение", &control, frame, &[])?;
                    let effects = flow
                        .observe(page(PageKind::Contact, "contact"), 3)
                        .map_err(|_| "contact")?;
                    show(
                        "контакты — звонок выключен",
                        &flow.control(),
                        frame,
                        &effects,
                    )?;
                    show(
                        "ошибка",
                        &flow.control(),
                        PresentationFrame {
                            failed: true,
                            repeat_alert: false,
                            ..frame
                        },
                        &[FlowEffect::Alert],
                    )?;
                    let effects = flow
                        .observe(page(PageKind::Booking, "booking"), 4)
                        .map_err(|_| "booking")?;
                    show(
                        "сита — управление человеку",
                        &flow.control(),
                        PresentationFrame {
                            active: false,
                            browser_retained: true,
                            ..frame
                        },
                        &effects,
                    )?;
                    show(
                        "звук отключён",
                        &flow.control(),
                        PresentationFrame {
                            active: false,
                            browser_retained: true,
                            sound_enabled: false,
                            ..frame
                        },
                        &[],
                    )?;
                    playback.stop().map_err(str::to_owned)?;
                    Ok(())
                })();
                println!("native_presentation_result={result:?}");
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("native presentation probe startup");
}
