//! Read-only document inspection; raw text is transient, never an event or a log.
//! Visible-frame traversal and response binding are the native port's responsibility.
use crate::bidi::{BidiConnection, ProtocolError};
use ceh_core::{model::DocumentId, recognition::PageSnapshot};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
/// No Debug: the snapshot can contain personal page text.
pub struct DocumentRead {
    pub realm: DocumentId,
    pub snapshot: PageSnapshot,
}
pub const INSPECT_DOCUMENT: &str = include_str!("inspect-document.js");
fn read_document_before(
    connection: &mut BidiConnection,
    context: &str,
    fill_mode: &str,
    deadline: Instant,
) -> Result<DocumentRead, ProtocolError> {
    if context.is_empty() || context.len() > 512 || !matches!(fill_mode, "manual" | "copy") {
        return Err(ProtocolError::InvalidData);
    }
    let value=connection.call_before("script.callFunction",json!({"target":{"context":context},"awaitPromise":false,"functionDeclaration":INSPECT_DOCUMENT,"arguments":[{"type":"string","value":fill_mode}]}),deadline)?;
    if value["type"] != "success" || value["result"]["type"] != "string" {
        return Err(ProtocolError::InvalidData);
    }
    let realm = DocumentId::new(value["realm"].as_str().ok_or(ProtocolError::InvalidData)?)
        .map_err(|_| ProtocolError::InvalidData)?;
    let raw = value["result"]["value"]
        .as_str()
        .filter(|s| s.len() <= 128 * 1024)
        .ok_or(ProtocolError::InvalidData)?;
    let snapshot = serde_json::from_str(raw).map_err(|_| ProtocolError::InvalidData)?;
    Ok(DocumentRead { realm, snapshot })
}

/// Resolve the exact iframe owner, including cross-origin children, without scrolling it.
/// If the parent navigated, its realm is destroyed and inspection fails closed.
fn frame_visible_before(
    connection: &mut BidiConnection,
    parent: &str,
    parent_realm: &DocumentId,
    child: &str,
    deadline: Instant,
) -> Result<bool, ProtocolError> {
    if parent.is_empty() || child.is_empty() || parent.len() > 512 || child.len() > 512 {
        return Err(ProtocolError::InvalidData);
    }
    let located=connection.call_before("browsingContext.locateNodes",json!({"context":parent,"locator":{"type":"context","value":{"context":child}},"maxNodeCount":2}),deadline)?;
    let nodes = located["nodes"]
        .as_array()
        .ok_or(ProtocolError::InvalidData)?;
    if nodes.len() != 1 {
        return Err(ProtocolError::InvalidData);
    }
    let shared = nodes[0]["sharedId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or(ProtocolError::InvalidData)?;
    let result=connection.call_before("script.callFunction",json!({"target":{"realm":parent_realm.as_str()},"awaitPromise":false,"arguments":[{"sharedId":shared}],"functionDeclaration":"el => { if (!el || !el.isConnected) return false; const style=getComputedStyle(el), box=el.getBoundingClientRect(); return style.visibility!=='hidden' && style.visibility!=='collapse' && box.width>0 && box.height>0; }"}),deadline)?;
    if result["type"] != "success" || result["result"]["type"] != "boolean" {
        return Err(ProtocolError::InvalidData);
    }
    result["result"]["value"]
        .as_bool()
        .ok_or(ProtocolError::InvalidData)
}

pub fn read_document(
    connection: &mut BidiConnection,
    context: &str,
    fill_mode: &str,
) -> Result<DocumentRead, ProtocolError> {
    read_document_before(
        connection,
        context,
        fill_mode,
        Instant::now() + Duration::from_secs(15),
    )
}
pub fn frame_visible(
    connection: &mut BidiConnection,
    parent: &str,
    parent_realm: &DocumentId,
    child: &str,
) -> Result<bool, ProtocolError> {
    frame_visible_before(
        connection,
        parent,
        parent_realm,
        child,
        Instant::now() + Duration::from_secs(15),
    )
}
/// Identity/source map contains no URL or page text. Snapshot remains transient.
pub struct FrameSource {
    pub context: String,
    pub realm: DocumentId,
}
pub struct PageRead {
    pub sources: Vec<FrameSource>,
    pub snapshot: PageSnapshot,
}
struct Inspection {
    sources: Vec<FrameSource>,
    snapshot: Option<PageSnapshot>,
    seen: usize,
    deadline: Instant,
}
impl Inspection {
    fn walk(
        &mut self,
        connection: &mut BidiConnection,
        node: &Value,
        parent: Option<&FrameSource>,
        fill_mode: &str,
        depth: usize,
    ) -> Result<(), ProtocolError> {
        self.seen += 1;
        if self.seen > 32 || depth > 8 || Instant::now() >= self.deadline {
            return Err(ProtocolError::InvalidData);
        }
        let context = node["context"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or(ProtocolError::InvalidData)?;
        if let Some(parent) = parent
            && !frame_visible_before(
                connection,
                &parent.context,
                &parent.realm,
                context,
                self.deadline,
            )?
        {
            return Ok(());
        }
        let document = read_document_before(connection, context, fill_mode, self.deadline)?;
        if document.snapshot.frames.len() != 1 {
            return Err(ProtocolError::InvalidData);
        }
        let source = FrameSource {
            context: context.to_owned(),
            realm: document.realm,
        };
        if let Some(main) = &mut self.snapshot {
            main.frames.extend(document.snapshot.frames);
        } else {
            self.snapshot = Some(document.snapshot);
        }
        self.sources.push(FrameSource {
            context: source.context.clone(),
            realm: source.realm.clone(),
        });
        let children = node["children"]
            .as_array()
            .ok_or(ProtocolError::InvalidData)?;
        for child in children {
            self.walk(connection, child, Some(&source), fill_mode, depth + 1)?;
        }
        Ok(())
    }
}
/// Read visible frames, bounded to32 contexts/depth8 and a shared15s response deadline.
/// A destroyed document invalidates the entire read; never act on a partial frame set.
pub fn read_page(
    connection: &mut BidiConnection,
    context: &str,
    fill_mode: &str,
) -> Result<PageRead, ProtocolError> {
    if context.is_empty() || context.len() > 512 {
        return Err(ProtocolError::InvalidData);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let tree = connection.call_before(
        "browsingContext.getTree",
        json!({"root":context,"maxDepth":8}),
        deadline,
    )?;
    let roots = tree["contexts"]
        .as_array()
        .ok_or(ProtocolError::InvalidData)?;
    if roots.len() != 1 || roots[0]["context"] != context {
        return Err(ProtocolError::InvalidData);
    }
    let mut inspection = Inspection {
        sources: Vec::new(),
        snapshot: None,
        seen: 0,
        deadline,
    };
    inspection.walk(connection, &roots[0], None, fill_mode, 0)?;
    for source in &inspection.sources {
        let alive=connection.call_before("script.evaluate",json!({"expression":"true","target":{"realm":source.realm.as_str()},"awaitPromise":false}),deadline)?;
        if alive["type"] != "success" || alive["result"]["value"] != true {
            return Err(ProtocolError::InvalidData);
        }
    }
    Ok(PageRead {
        sources: inspection.sources,
        snapshot: inspection.snapshot.ok_or(ProtocolError::InvalidData)?,
    })
}
