//! The download of an update running in the background: its progress, its result, and the one thing the
//! person can do with it (open the verified package). Nothing is installed by this app itself.
use ceh_services::updates::UpdateError;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Downloading,
    Ready,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Status {
    pub phase: Phase,
    pub percent: u8,
    pub error: Option<&'static str>,
}
#[derive(Default)]
struct Inner {
    phase: Option<Phase>,
    percent: u8,
    error: Option<&'static str>,
    path: Option<PathBuf>,
}
#[derive(Clone, Default)]
pub struct UpdateJob {
    inner: Arc<Mutex<Inner>>,
    cancel: Arc<AtomicBool>,
}
impl UpdateJob {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn status(&self) -> Status {
        let inner = self.inner.lock().expect("update state");
        Status { phase: inner.phase.unwrap_or(Phase::Idle), percent: inner.percent, error: inner.error }
    }
    /// The verified package, only while a finished download is current.
    pub fn install_path(&self) -> Option<PathBuf> {
        let inner = self.inner.lock().expect("update state");
        (inner.phase == Some(Phase::Ready)).then(|| inner.path.clone()).flatten()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    /// Run `work` on its own thread; `false` while another download runs.
    pub fn start_with<F>(&self, work: F) -> bool
    where
        F: FnOnce(&AtomicBool, &mut dyn FnMut(u8)) -> Result<PathBuf, UpdateError> + Send + 'static,
    {
        {
            let mut inner = self.inner.lock().expect("update state");
            if inner.phase == Some(Phase::Downloading) {
                return false;
            }
            *inner = Inner { phase: Some(Phase::Downloading), ..Inner::default() };
        }
        self.cancel.store(false, Ordering::Release);
        let (state, cancel) = (self.inner.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let progress_state = state.clone();
            let result = work(&cancel, &mut |percent| {
                if let Ok(mut inner) = progress_state.lock() {
                    inner.percent = percent;
                }
            });
            let mut inner = state.lock().expect("update state");
            match result {
                Ok(path) => {
                    inner.phase = Some(Phase::Ready);
                    inner.percent = 100;
                    inner.path = Some(path);
                }
                Err(error) => {
                    inner.phase = Some(Phase::Failed);
                    inner.error = Some(match error {
                        UpdateError::Cancelled => "cancelled",
                        UpdateError::Network => "network",
                        UpdateError::Invalid => "verification_failed",
                    });
                }
            }
        });
        true
    }
    /// Download and verify the latest release for this system into `dir`.
    pub fn start(&self, current: &'static str, dir: PathBuf) -> bool {
        self.start_with(move |cancel, progress| ceh_services::updates::download_latest(current, &dir, cancel, progress))
    }
}
