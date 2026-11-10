//! Native observations from read-only DOM plus exact loaded-navigation HTTP facts.
use crate::{
    bidi::{BidiConnection, ProtocolError},
    dom::{PageRead, read_page},
    navigation::NavigationFacts,
};
use ceh_core::{
    model::{ActionKind, Observation, PageKind, RequestMethod},
    recognition::{Recognition, recognize},
};
use serde_json::json;
/// No Debug: raw DOM text is transient and may contain private user data.
pub struct ObservedPage {
    pub observation: Observation,
    pub recognition: Recognition,
    pub page: PageRead,
}
pub struct PageObserver {
    context: String,
    facts: NavigationFacts,
    last_action: Option<ActionKind>,
    pending_action: Option<ActionKind>,
    passive: Option<crate::passive::ResponseQueue>,
    /// Requests of this context started and not yet completed or failed (the warm-up's `networkidle`).
    inflight: u32,
}
impl PageObserver {
    pub fn new(context: &str) -> Result<Self, ProtocolError> {
        if context.is_empty() || context.len() > 512 {
            return Err(ProtocolError::InvalidData);
        }
        Ok(Self {
            context: context.to_owned(),
            facts: NavigationFacts::default(),
            last_action: None,
            pending_action: None,
            passive: None,
            inflight: 0,
        })
    }
    /// The flow engine supplies action context; DOM text cannot invent sent actions.
    pub fn set_actions(&mut self, last: Option<ActionKind>, pending: Option<ActionKind>) {
        self.last_action = last;
        self.pending_action = pending;
    }
    /// Feed events from the same owned connection when a central protocol pump drains it.
    /// Do not accept this input from UI IPC or page JavaScript.
    pub fn ingest(&mut self, event: &serde_json::Value) -> Result<(), ProtocolError> {
        if let Some(queue) = &mut self.passive {
            queue.ingest(event);
        }
        let ours = event["params"]["context"].as_str() == Some(self.context.as_str());
        match event["method"].as_str() {
            Some("network.beforeRequestSent") if ours => self.inflight = self.inflight.saturating_add(1),
            Some("network.responseCompleted") if ours => self.inflight = self.inflight.saturating_sub(1),
            Some("browsingContext.navigationStarted") if ours => self.inflight = 0,
            Some("network.fetchError") => {
                if ours {
                    self.inflight = self.inflight.saturating_sub(1);
                }
                return Ok(());
            }
            _ => {}
        }
        self.facts
            .ingest(event)
            .map_err(|_| ProtocolError::InvalidData)
    }
    /// No request of the page is in flight (Python waits for `networkidle` during the F5 warm-up).
    pub fn network_quiet(&self) -> bool {
        self.inflight == 0
    }
    /// Enable only after the debug/capture policy and collector lifecycle allow it.
    pub fn set_passive_collection(&mut self, enabled: bool) -> Result<(), ProtocolError> {
        if enabled {
            if self.passive.is_none() {
                self.passive = Some(crate::passive::ResponseQueue::new(&self.context)?);
            }
        } else {
            self.passive = None;
        }
        Ok(())
    }
    pub fn take_responses(&mut self) -> crate::passive::ResponseBatch {
        self.passive
            .as_mut()
            .map_or_else(Default::default, crate::passive::ResponseQueue::drain)
    }
    fn drain(&mut self, connection: &mut BidiConnection) -> Result<(), ProtocolError> {
        for event in connection.take_events() {
            self.ingest(&event)?;
        }
        Ok(())
    }
    pub fn observe(
        &mut self,
        connection: &mut BidiConnection,
        fill_mode: &str,
    ) -> Result<ObservedPage, ProtocolError> {
        // A harmless round-trip drains previously queued protocol events before the read.
        connection.call(
            "script.evaluate",
            json!({"expression":"true","target":{"context":self.context},"awaitPromise":false}),
        )?;
        self.drain(connection)?;
        let before = self.facts.clone();
        let mut page = read_page(connection, &self.context, fill_mode)?;
        self.drain(connection)?;
        let stable = page.sources.iter().all(|source| {
            let current = self.facts.current_navigation(&source.context);
            let same = before.current_navigation(&source.context) == current;
            // Blank/srcdoc children may have no HTTP navigation at all. Their exact
            // realms were validated; do not invent GET or wait forever for a load ID.
            let ready = self.facts.is_loaded(&source.context)
                || (source.context != self.context && current.is_none());
            same && ready
        });
        if stable {
            for (source, frame) in page.sources.iter().zip(&mut page.snapshot.frames) {
                if let Some(navigation) = self.facts.current_navigation(&source.context) {
                    frame.status = self.facts.get(&source.context, navigation).status;
                    frame.response_matches = true;
                }
            }
        }
        page.snapshot.last_action = action_text(self.last_action)?;
        page.snapshot.pending_action = action_text(self.pending_action)?;
        let recognition = recognize(&page.snapshot).map_err(|_| ProtocolError::InvalidData)?;
        // A visible booking always yields to the person promptly; all automatic actions
        // wait for a stable loaded document. Never guess GET from a DOM snapshot.
        let kind = if stable || recognition.kind == PageKind::Booking {
            recognition.kind
        } else {
            PageKind::Loading
        };
        let method = if stable {
            self.facts
                .current_navigation(&self.context)
                .map(|nav| self.facts.get(&self.context, nav).method)
                .unwrap_or_default()
        } else {
            RequestMethod::Unknown
        };
        let observation = Observation {
            document: page
                .sources
                .first()
                .ok_or(ProtocolError::InvalidData)?
                .realm
                .clone(),
            kind,
            method,
            operation_code: kind == recognition.kind && recognition.operation_code,
            variant: if kind == recognition.kind {
                recognition.variant.clone()
            } else {
                String::new()
            },
        };
        Ok(ObservedPage {
            observation,
            recognition,
            page,
        })
    }
}

fn action_text(action: Option<ActionKind>) -> Result<Option<String>, ProtocolError> {
    let Some(action) = action else {
        return Ok(None);
    };
    let value = serde_json::to_value(action).map_err(|_| ProtocolError::InvalidData)?;
    Ok(Some(
        value.as_str().ok_or(ProtocolError::InvalidData)?.to_owned(),
    ))
}
