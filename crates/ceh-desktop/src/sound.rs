//! Local sound resource and cancellable native playback; repetition belongs to the monotonic timer.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sound_resource_is_private_idempotent_and_never_overwrites_an_unexpected_file() {
        let root = tempfile::tempdir().unwrap();
        let path = ensure_sound(root.path()).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(original.starts_with(b"RIFF"));
        assert_eq!(ensure_sound(root.path()).unwrap(), path);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::write(&path, b"unexpected").unwrap();
        assert!(ensure_sound(root.path()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"unexpected");
    }
    #[cfg(unix)]
    #[test]
    fn stop_reaps_an_active_player_instead_of_waiting_for_the_sound_to_finish() {
        let child = std::process::Command::new("/bin/sleep")
            .arg("10")
            .spawn()
            .unwrap();
        let mut child = Some(child);
        let started = std::time::Instant::now();
        let status = stop_child(&mut child).unwrap().unwrap();
        assert!(!status.success());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(child.is_none());
        assert!(stop_child(&mut child).unwrap().is_none());
    }
}

const SOUND: &[u8] = include_bytes!("../../../app/desktop/assets/attention.wav");
pub fn ensure_sound(root: &std::path::Path) -> Result<std::path::PathBuf, &'static str> {
    let directory = root.join("data");
    if directory
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
    {
        return Err("sound_unavailable");
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&directory)
        .map_err(|_| "sound_unavailable")?;
    let path = directory.join("attention.wav");
    match std::fs::symlink_metadata(&path) {
        Ok(m) => {
            use std::io::Read;
            if !m.is_file() || m.len() != SOUND.len() as u64 {
                return Err("sound_unavailable");
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .map_err(|_| "sound_unavailable")?
                .take(SOUND.len() as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "sound_unavailable")?;
            if bytes != SOUND {
                return Err("sound_unavailable");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::achievement_exports::write_file(&path, SOUND).map_err(|_| "sound_unavailable")?
        }
        Err(_) => return Err("sound_unavailable"),
    }
    Ok(path)
}
#[cfg(not(windows))]
fn stop_child(
    child: &mut Option<std::process::Child>,
) -> Result<Option<std::process::ExitStatus>, &'static str> {
    if let Some(mut process) = child.take() {
        let _ = process.kill();
        return process.wait().map(Some).map_err(|_| "sound_unavailable");
    };
    Ok(None)
}
#[derive(Default)]
pub struct SoundPlayback {
    #[cfg(not(windows))]
    child: Option<std::process::Child>,
    #[cfg(windows)]
    playing: bool,
}
impl SoundPlayback {
    pub fn play(&mut self, root: &std::path::Path) -> Result<(), &'static str> {
        self.stop()?;
        let path = ensure_sound(root)?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // Valid, NUL-terminated UTF-16 path lives through the synchronous API entry; playback is process-local.
            if unsafe {
                PlaySoundW(
                    name.as_ptr(),
                    std::ptr::null_mut(),
                    0x0002_0000 | 0x0001 | 0x0002,
                )
            } == 0
            {
                return Err("sound_unavailable");
            }
            self.playing = true;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            let commands: &[(&str, &[&str])] = &[("/usr/bin/afplay", &[])];
            #[cfg(not(target_os = "macos"))]
            let commands: &[(&str, &[&str])] = &[("paplay", &[]), ("aplay", &["-q"])];
            for (program, args) in commands {
                match std::process::Command::new(program)
                    .args(*args)
                    .arg(&path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(child) => {
                        self.child = Some(child);
                        return Ok(());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err("sound_unavailable"),
                }
            }
            Err("sound_unavailable")
        }
    }
    pub fn stop(&mut self) -> Result<(), &'static str> {
        #[cfg(windows)]
        {
            if self.playing {
                // NULL stops only waveform playback in this process, not another application.
                unsafe {
                    PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
                }
                self.playing = false;
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            stop_child(&mut self.child).map(|_| ())
        }
    }
}
impl Drop for SoundPlayback {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[cfg(windows)]
#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
}
