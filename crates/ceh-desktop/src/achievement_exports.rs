//! Explicit local export actions. Public models never contain appointment details.
use serde_json::{Value, json};
use std::path::Path;
fn language(value: &str) -> Result<(), &'static str> {
    if ["ru", "en", "es"].contains(&value) {
        Ok(())
    } else {
        Err("export_invalid")
    }
}
pub fn public_model(root: &Path, id: &str, key: &str, lang: &str) -> Result<Value, &'static str> {
    language(lang)?;
    let record = crate::appointment::achievement(root, id, key)?;
    Ok(
        json!({"layouts":{"dark":ceh_services::exports::card_layout(&record,lang,true),"light":ceh_services::exports::card_layout(&record,lang,false)},"post":ceh_services::exports::public_post(&record,lang,true)}),
    )
}
pub fn calendar_model(root: &Path, id: &str, key: &str) -> Result<Value, &'static str> {
    let record = crate::appointment::achievement(root, id, key)?;
    let url =
        ceh_services::exports::google_calendar_url(&record).map_err(|_| "appointment_invalid")?;
    let data = &record["appointment"];
    Ok(
        json!({"date":data["date"],"time":data["time"],"office":data["office"],"address":data["address"],"google_url":url}),
    )
}
pub fn calendar_bytes(root: &Path, id: &str, key: &str) -> Result<Vec<u8>, &'static str> {
    let record = crate::appointment::achievement(root, id, key)?;
    ceh_services::exports::calendar(&record, crate::current_time()?)
        .map(String::into_bytes)
        .map_err(|_| "appointment_invalid")
}
/// Decode only the fixed public-card dimensions, with bounded allocation and no metadata export.
pub fn decode_card(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("image_invalid");
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 8 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|_| "image_invalid")?;
    let info = reader.info();
    if info.width != 1200
        || info.height != 1240
        || info.bit_depth != png::BitDepth::Eight
        || !matches!(info.color_type, png::ColorType::Rgb | png::ColorType::Rgba)
    {
        return Err("image_invalid");
    }
    let mut output = vec![0; reader.output_buffer_size().ok_or("image_invalid")?];
    let frame = reader
        .next_frame(&mut output)
        .map_err(|_| "image_invalid")?;
    if frame.color_type == png::ColorType::Rgba {
        output.truncate(frame.buffer_size());
        Ok(output)
    } else {
        Ok(output[..frame.buffer_size()]
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect())
    }
}
/// Atomic write following an explicit native Save dialog; caller never supplies this path over IPC.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    use std::io::Write;
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("export_failed");
    }
    let parent = path.parent().ok_or("export_failed")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| "export_failed")?;
    file.write_all(bytes).map_err(|_| "export_failed")?;
    file.as_file().sync_all().map_err(|_| "export_failed")?;
    file.persist(path).map_err(|_| "export_failed")?;
    Ok(())
}
#[cfg(feature = "shell")]
pub fn copy_image(bytes: &[u8]) -> Result<(), &'static str> {
    use std::sync::{LazyLock, Mutex};
    static CLIPBOARD: LazyLock<Mutex<Option<arboard::Clipboard>>> =
        LazyLock::new(|| Mutex::new(None));
    let rgba = decode_card(bytes)?;
    let mut clipboard = CLIPBOARD.lock().map_err(|_| "clipboard_unavailable")?;
    if clipboard.is_none() {
        *clipboard = Some(arboard::Clipboard::new().map_err(|_| "clipboard_unavailable")?);
    }
    clipboard
        .as_mut()
        .ok_or("clipboard_unavailable")?
        .set_image(arboard::ImageData {
            width: 1200,
            height: 1240,
            bytes: std::borrow::Cow::Owned(rgba),
        })
        .map_err(|_| "clipboard_unavailable")
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Telegram,
    X,
    Facebook,
    Instagram,
    Reddit,
}
#[cfg(feature = "shell")]
impl Network {
    fn name(&self) -> &str {
        match self {
            Self::Telegram => "telegram",
            Self::X => "x",
            Self::Facebook => "facebook",
            Self::Instagram => "instagram",
            Self::Reddit => "reddit",
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExportAction {
    Google,
    SaveCalendar,
    OpenCalendar,
    SaveImage { png: Vec<u8> },
    CopyImage { png: Vec<u8> },
    PrepareText { network: Network, text: String },
    PrepareImage { network: Network, png: Vec<u8> },
    ShowImage,
}
#[cfg(feature = "shell")]
#[derive(Default)]
pub struct NativeExports {
    saved_calendars:
        std::sync::Mutex<std::collections::HashMap<(String, String), std::path::PathBuf>>,
    operations: std::sync::Mutex<()>,
}
#[cfg(feature = "shell")]
impl NativeExports {
    pub fn execute(
        &self,
        window: &tauri::WebviewWindow,
        root: &Path,
        id: &str,
        key: &str,
        lang: &str,
        request: ExportAction,
    ) -> Result<Value, &'static str> {
        use crate::notification_actions::{clipboard, open};
        language(lang)?;
        crate::appointment::achievement(root, id, key)?;
        let _operation = self.operations.try_lock().map_err(|_| "export_busy")?;
        let open_target = |target: &std::ffi::OsStr| open(target).map_err(|_| "open_failed");
        let image_dir = || -> Result<std::path::PathBuf, &'static str> {
            let data = root.join("data");
            let path = data.join("share");
            for candidate in [&data, &path] {
                if candidate
                    .symlink_metadata()
                    .is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
                {
                    return Err("export_failed");
                }
            }
            std::fs::create_dir_all(&path).map_err(|_| "export_failed")?;
            Ok(path)
        };
        match request {
            ExportAction::Google => {
                let value = calendar_model(root, id, key)?;
                open_target(std::ffi::OsStr::new(
                    value["google_url"].as_str().ok_or("appointment_invalid")?,
                ))?;
                Ok(json!({"result":"google_opened"}))
            }
            ExportAction::SaveCalendar => {
                let bytes = calendar_bytes(root, id, key)?;
                let title = match lang {
                    "ru" => "Сохранить .ics",
                    "es" => "Guardar .ics",
                    _ => "Save .ics",
                };
                let Some(path) = rfd::FileDialog::new()
                    .set_parent(window)
                    .set_title(title)
                    .set_file_name("cita-appointment.ics")
                    .add_filter("iCalendar", &["ics"])
                    .save_file()
                else {
                    return Ok(json!({"result":"cancelled"}));
                };
                write_file(&path, &bytes)?;
                let mut paths = self.saved_calendars.lock().map_err(|_| "export_failed")?;
                if paths.len() >= 32 {
                    paths.clear();
                }
                paths.insert((id.into(), key.into()), path);
                Ok(json!({"result":"calendar_saved"}))
            }
            ExportAction::OpenCalendar => {
                let paths = self.saved_calendars.lock().map_err(|_| "export_failed")?;
                let path = paths.get(&(id.into(), key.into())).ok_or("export_failed")?;
                open_target(path.as_os_str())?;
                Ok(Value::Null)
            }
            ExportAction::SaveImage { png } => {
                let bytes = clean_png(&png)?;
                let title = match lang {
                    "ru" => "Сохранить PNG",
                    "es" => "Guardar PNG",
                    _ => "Save PNG",
                };
                let Some(path) = rfd::FileDialog::new()
                    .set_parent(window)
                    .set_title(title)
                    .set_file_name("cita-achievement.png")
                    .add_filter("PNG", &["png"])
                    .save_file()
                else {
                    return Ok(json!({"result":"cancelled"}));
                };
                write_file(&path, &bytes)?;
                Ok(json!({"result":"image_saved"}))
            }
            ExportAction::CopyImage { png } => {
                copy_image(&png)?;
                Ok(json!({"result":"image_copied"}))
            }
            ExportAction::PrepareText { network, text } => {
                if text.chars().count() > 16384 {
                    return Err("export_invalid");
                }
                clipboard(&text).map_err(|_| "clipboard_unavailable")?;
                open_target(std::ffi::OsStr::new(
                    ceh_services::exports::share_url(network.name()).ok_or("export_invalid")?,
                ))?;
                Ok(json!({"result":"text_prepared"}))
            }
            ExportAction::PrepareImage { network, png } => {
                let bytes = clean_png(&png)?;
                write_file(&image_dir()?.join("cita-achievement.png"), &bytes)?;
                open_target(std::ffi::OsStr::new(
                    ceh_services::exports::share_url(network.name()).ok_or("export_invalid")?,
                ))?;
                Ok(json!({"result":"image_prepared"}))
            }
            ExportAction::ShowImage => {
                open_target(image_dir()?.as_os_str())?;
                Ok(Value::Null)
            }
        }
    }
}
/// Re-encoding removes any client-supplied PNG text/metadata from the shared file.
pub fn clean_png(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let pixels = decode_card(bytes)?;
    let mut result = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut result, 1200, 1240);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|_| "image_invalid")?
            .write_image_data(&pixels)
            .map_err(|_| "image_invalid")?;
    }
    Ok(result)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicLink {
    Github,
    Readme,
    Issues,
    Releases,
    Firefox,
    Chrome,
    Brave,
    Edge,
    Sponsor,
    Crypto,
}
pub fn public_link(target: PublicLink, lang: &str) -> Result<String, &'static str> {
    language(lang)?;
    let repo = "https://github.com/itwormz/extranjeria-cita-helper";
    Ok(match target {
        PublicLink::Github => repo.into(),
        PublicLink::Readme => format!(
            "{repo}/blob/main/{}",
            match lang {
                "ru" => "README.ru.md",
                "es" => "README.es.md",
                _ => "README.md",
            }
        ),
        PublicLink::Issues => format!("{repo}/issues/new"),
        PublicLink::Firefox => "https://www.mozilla.org/firefox/new/".into(),
        PublicLink::Chrome => "https://www.google.com/chrome/".into(),
        PublicLink::Brave => "https://brave.com/download/".into(),
        PublicLink::Edge => "https://www.microsoft.com/edge/download".into(),
        PublicLink::Releases => format!("{repo}/releases/latest"),
        PublicLink::Sponsor => "https://github.com/sponsors/itwormz".into(),
        PublicLink::Crypto => "https://nowpayments.io/donation/itworm".into(),
    })
}
