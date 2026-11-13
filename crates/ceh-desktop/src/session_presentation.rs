//! Trusted session event→native presentation. No browser commands or UI IPC.
use crate::presentation::{AttentionTimer, IconStatus, StatusFacts, icon_status};
use ceh_core::{
    control::{Authority, ControlMode, ControlSnapshot},
    flow::FlowEffect,
    model::SessionId,
};
#[derive(Clone, Copy)]
pub struct PresentationFrame {
    pub active: bool,
    pub failed: bool,
    pub browser_retained: bool,
    pub sound_enabled: bool,
    /// Certificate reminder is once; manual office/booking attention can repeat.
    pub repeat_alert: bool,
}
impl Default for PresentationFrame {
    fn default() -> Self {
        Self {
            active: false,
            failed: false,
            browser_retained: false,
            sound_enabled: false,
            repeat_alert: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCommand {
    None,
    Play,
    Stop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationChange {
    pub icon: Option<IconStatus>,
    pub sound: SoundCommand,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationError {
    StaleEvent,
    ClockReversed,
    InvalidControl,
}
pub struct SessionPresentation {
    authority: Authority,
    sequence: Option<u64>,
    last_time: Option<u64>,
    icon: Option<IconStatus>,
    timer: AttentionTimer,
    alarm: Option<bool>,
    enabled: bool,
    playing: bool,
}
impl SessionPresentation {
    pub fn new(session: SessionId, interval_ms: u64) -> Self {
        Self {
            authority: Authority::new(session),
            sequence: None,
            last_time: None,
            icon: None,
            timer: AttentionTimer::new(interval_ms),
            alarm: None,
            enabled: false,
            playing: false,
        }
    }
    pub fn set_alert_interval(&mut self, interval_ms: u64) { self.timer.set_interval(interval_ms); }
    /// Controller supplies an increasing sequence and monotonic processing time.
    /// Rejected updates are atomic and cannot resurrect sound after pause/handoff.
    pub fn update(
        &mut self,
        control: &ControlSnapshot,
        sequence: u64,
        now: u64,
        frame: PresentationFrame,
        effects: &[FlowEffect],
    ) -> Result<PresentationChange, PresentationError> {
        if self.sequence.is_some_and(|old| sequence <= old) {
            return Err(PresentationError::StaleEvent);
        }
        if self.last_time.is_some_and(|old| now < old) {
            return Err(PresentationError::ClockReversed);
        }
        self.authority
            .apply(control)
            .map_err(|_| PresentationError::InvalidControl)?;
        self.sequence = Some(sequence);
        self.last_time = Some(now);
        let mode = self.authority.mode();
        let has_session =
            frame.active || frame.browser_retained || frame.failed || mode == ControlMode::Manual;
        for effect in effects {
            match effect {
                FlowEffect::Silence => {
                    self.alarm = None;
                    self.timer.stop();
                }
                FlowEffect::Alert if mode != ControlMode::Paused && has_session => {
                    // Each accepted sequence carries new core effects exactly
                    // once; ordinary polls carry no Alert and cannot rearm it.
                    self.alarm = Some(frame.repeat_alert);
                    self.timer.start(now, frame.repeat_alert);
                }
                _ => {}
            }
        }
        if !has_session {
            self.alarm = None;
            self.timer.stop();
        }
        let enabled = frame.sound_enabled && mode != ControlMode::Paused && has_session;
        if !enabled {
            self.timer.stop();
        } else if !self.enabled && self.alarm == Some(true) {
            self.timer.start(now, true);
        }
        self.enabled = enabled;
        let sound = if enabled && self.alarm.is_some() && self.timer.due(now) {
            self.playing = true;
            SoundCommand::Play
        } else if self.playing && (!enabled || self.alarm.is_none()) {
            self.playing = false;
            SoundCommand::Stop
        } else {
            SoundCommand::None
        };
        let facts = StatusFacts {
            failed: frame.failed,
            browser_retained: frame.browser_retained,
            booking_handoff: mode == ControlMode::Manual,
            paused: mode == ControlMode::Paused,
            mode: if mode == ControlMode::Paused {
                "paused"
            } else if self.alarm.is_some() {
                "manual"
            } else {
                "working"
            }
            .into(),
            ..Default::default()
        };
        let status = icon_status(&facts, frame.active, false);
        let icon = if self.icon == Some(status) {
            None
        } else {
            self.icon = Some(status);
            Some(status)
        };
        Ok(PresentationChange { icon, sound })
    }
}
/// Call on the owned runtime worker, never while holding the browser/control
/// authority mutex. Sound cancellation and icon failures are independent.
#[cfg(feature = "shell")]
pub fn apply_native(
    window: &tauri::WebviewWindow,
    root: &std::path::Path,
    player: &mut crate::sound::SoundPlayback,
    change: PresentationChange,
) -> Result<(), &'static str> {
    let sound = match change.sound {
        SoundCommand::None => Ok(()),
        SoundCommand::Play => player.play(root),
        SoundCommand::Stop => player.stop(),
    };
    let icon = change.icon.map_or(Ok(()), |status| {
        crate::presentation::apply_icon(window, status)
    });
    sound.and(icon)
}
