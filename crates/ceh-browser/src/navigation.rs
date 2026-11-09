//! Per-navigation HTTP facts. Never retain URLs, headers, bodies or applicant values.
use ceh_core::model::RequestMethod;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseFacts {
    pub method: RequestMethod,
    pub status: Option<u16>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactsError {
    InvalidEvent,
    Capacity,
}
#[derive(Clone)]
struct Document {
    navigation: String,
    request: Option<String>,
    facts: ResponseFacts,
    loaded: bool,
}
/// One tracker per owned protocol session; destroyed contexts are forgotten.
#[derive(Default, Clone)]
pub struct NavigationFacts {
    documents: BTreeMap<String, Document>,
}
fn id(value: &Value) -> Result<&str, FactsError> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .ok_or(FactsError::InvalidEvent)
}
impl NavigationFacts {
    pub fn ingest(&mut self, event: &Value) -> Result<(), FactsError> {
        let method = event["method"].as_str().unwrap_or("");
        if !matches!(
            method,
            "browsingContext.navigationStarted"
                | "browsingContext.contextDestroyed"
                | "network.beforeRequestSent"
                | "network.responseCompleted"
                | "browsingContext.load"
        ) {
            return Ok(());
        }
        let params = &event["params"];
        // Worker/subresource requests have no navigation, and may have no context.
        if method.starts_with("network.") && params["navigation"].is_null() {
            return Ok(());
        }
        let context = id(&params["context"])?;
        if method == "browsingContext.contextDestroyed" {
            self.documents.remove(context);
            return Ok(());
        }
        // Subresource requests have null navigation and must not change document facts.
        if params["navigation"].is_null() {
            return Ok(());
        }
        let navigation = id(&params["navigation"])?;
        if method == "browsingContext.navigationStarted" {
            if !self.documents.contains_key(context) && self.documents.len() >= 64 {
                return Err(FactsError::Capacity);
            }
            self.documents.insert(
                context.to_owned(),
                Document {
                    navigation: navigation.to_owned(),
                    request: None,
                    facts: ResponseFacts::default(),
                    loaded: false,
                },
            );
            return Ok(());
        }
        let Some(document) = self
            .documents
            .get_mut(context)
            .filter(|d| d.navigation == navigation)
        else {
            return Ok(());
        };
        if method == "browsingContext.load" {
            document.loaded = true;
            return Ok(());
        }
        let request = id(&params["request"]["request"])?;
        if method == "network.beforeRequestSent" {
            document.request = Some(request.to_owned());
            document.facts = ResponseFacts {
                method: match params["request"]["method"].as_str() {
                    Some("GET") => RequestMethod::Get,
                    Some("POST") => RequestMethod::Post,
                    _ => RequestMethod::Unknown,
                },
                status: None,
            };
        } else if document.request.as_deref() == Some(request) {
            let status = params["response"]["status"]
                .as_u64()
                .filter(|s| (100..=599).contains(s))
                .ok_or(FactsError::InvalidEvent)?;
            document.facts.status = Some(status as u16);
        }
        Ok(())
    }
    pub fn current_navigation(&self, context: &str) -> Option<&str> {
        self.documents.get(context).map(|d| d.navigation.as_str())
    }
    pub fn is_loaded(&self, context: &str) -> bool {
        self.documents.get(context).is_some_and(|d| d.loaded)
    }
    pub fn get(&self, context: &str, navigation: &str) -> ResponseFacts {
        self.documents
            .get(context)
            .filter(|d| d.navigation == navigation && d.loaded)
            .map(|d| d.facts)
            .unwrap_or_default()
    }
}
