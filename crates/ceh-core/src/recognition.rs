//! Python-compatible page priority from already-visible document facts.
//! Snapshot text is transient and must not be persisted as a public event.
use crate::{model::PageKind, python_unicode::is_space};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
pub struct VisibleFrame {
    pub text: String,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default = "yes")]
    pub response_matches: bool,
}
fn yes() -> bool {
    true
}
fn editable() -> bool {
    true
}
fn endpoint() -> String {
    "acCitar".to_owned()
}
fn fill_mode() -> String {
    "manual".to_owned()
}

#[derive(Default, Deserialize)]
pub struct PageShape {
    pub forms: u64,
    pub selects: u64,
    pub buttons: u64,
    pub inputs: [u64; 7],
}
#[derive(Deserialize)]
pub struct PageSnapshot {
    pub visible: Vec<String>,
    pub frames: Vec<VisibleFrame>,
    #[serde(default = "yes")]
    pub official: bool,
    #[serde(default = "endpoint")]
    pub endpoint: String,
    #[serde(default = "editable")]
    pub applicant_editable: bool,
    #[serde(default = "fill_mode")]
    pub fill_mode: String,
    #[serde(default)]
    pub button_label: String,
    #[serde(default)]
    pub last_action: Option<String>,
    #[serde(default)]
    pub pending_action: Option<String>,
    #[serde(default)]
    pub shape: PageShape,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recognition {
    pub kind: PageKind,
    pub operation_code: bool,
    pub certificate_validation_error: bool,
    pub recovery_action_first: bool,
    pub source_frame: usize,
    pub variant: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidSnapshot;

/// The phrases of a WAF block page (Python `_challenge_code`), most specific first.
const BLOCK_MARKERS: [&str; 8] = [
    "access denied",
    "request rejected",
    "requested url was rejected",
    "forbidden",
    "too many requests",
    "demasiadas solicitudes",
    "intrusion prevention triggered",
    "attempt to access the internet resource is blocked",
];
/// Which block phrase a page text contains, if any: the name goes into the diagnostics, the page text never.
pub fn block_marker(text: &str) -> Option<&'static str> {
    let text = normalize(text);
    BLOCK_MARKERS.into_iter().find(|marker| text.contains(marker))
}
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split(is_space)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn has_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}
fn certificate_error(text: &str) -> bool {
    text.contains("dssafirmaverifycertificate")
        || (text.contains("error al validar el certificado") && has_any(text, &["saml", "@firma"]))
}
fn dead_kind(text: &str) -> Option<PageKind> {
    if has_any(
        text,
        &[
            "sesión caducada",
            "sesion caducada",
            "sesión ha caducado",
            "sesion ha caducado",
        ],
    ) {
        return Some(PageKind::Expired);
    }
    let dead_document = matches!(
        text,
        "problema al cargar la página"
            | "problem loading page"
            | "server not found"
            | "la conexión ha caducado"
    );
    let server = has_any(
        text,
        &[
            "500 internal server error",
            "http error 500",
            "http 500",
            "error 500",
            "status code 500",
            "error interno del servidor",
            "internal server error",
            "the server encountered an internal error",
            "bad gateway",
            "http error 502",
            "http 502",
            "error 502",
            "status code 502",
            "service unavailable",
            "servicio no disponible",
            "http error 503",
            "http 503",
            "error 503",
            "status code 503",
            "gateway time-out",
            "gateway timeout",
            "http error 504",
            "http 504",
            "error 504",
            "status code 504",
            "error de puerta de enlace",
            "se ha producido un error en el sistema",
            "por favor inténtelo de nuevo",
            "por favor intentelo de nuevo",
        ],
    ) || (text.contains("se ha producido un error")
        && text.contains("reinténtelo de nuevo más tarde"));
    if dead_document || server || certificate_error(text) {
        Some(PageKind::ServerError)
    } else {
        None
    }
}
/// Classify a bounded visible recovery region with the same priority as a page.
/// Text is transient and must never become a public log/statistics field.
pub fn recovery_message_kind(text: &str) -> Option<PageKind> {
    if text.len() > 65536 {
        return None;
    }
    dead_kind(&normalize(text))
}
fn shape_hash(shape: &PageShape) -> String {
    // Python json.dumps(sort_keys=True): ordering and spaces are part of the legacy hash.
    let inputs = shape
        .inputs
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let raw = format!(
        "{{\"buttons\": {}, \"forms\": {}, \"inputs\": [{}], \"selects\": {}}}",
        shape.buttons, shape.forms, inputs, shape.selects
    );
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

pub fn recognize(snapshot: &PageSnapshot) -> Result<Recognition, InvalidSnapshot> {
    if snapshot.frames.is_empty() {
        return Err(InvalidSnapshot);
    }
    let mut result = Recognition {
        kind: PageKind::Unknown,
        operation_code: false,
        certificate_validation_error: false,
        recovery_action_first: false,
        source_frame: 0,
        variant: String::new(),
    };
    if let Some(index) = snapshot
        .frames
        .iter()
        .position(|frame| frame.response_matches && frame.status == Some(429))
    {
        result.kind = PageKind::RateLimited;
        result.source_frame = index;
        return Ok(result);
    }
    for (index, frame) in snapshot.frames.iter().enumerate() {
        let text = normalize(&frame.text);
        let dead = dead_kind(&text).or_else(|| {
            if frame.response_matches && frame.status.is_some_and(|status| status >= 500) {
                Some(PageKind::ServerError)
            } else {
                None
            }
        });
        if let Some(kind) = dead {
            result.kind = kind;
            result.source_frame = index;
            result.certificate_validation_error =
                kind == PageKind::ServerError && certificate_error(&text);
            if kind == PageKind::Expired {
                result.recovery_action_first = text.contains(
                    "la provincia seleccionada no ofrece el servicio de cita previa internet",
                );
                return Ok(result);
            }
        }
    }
    if result.kind != PageKind::Unknown {
        return Ok(result);
    }
    // Python raises HTTP_BLOCKED for a 403 entry response after the challenge-text check (captcha text
    // wins). Judged before the body check: an empty 403 body has no height and would look like loading.
    let entry = &snapshot.frames[0];
    if entry.response_matches
        && entry.status == Some(403)
        && !has_any(&normalize(&entry.text), &["captcha", "código de la imagen"])
    {
        result.kind = PageKind::Blocked;
        result.source_frame = 0;
        return Ok(result);
    }
    let visible = |selector: &str| snapshot.visible.iter().any(|s| s == selector);
    if !visible("body") {
        result.kind = PageKind::Loading;
        return Ok(result);
    }
    let text = normalize(&snapshot.frames[0].text);
    let no_slots = has_any(
        &text,
        &[
            "en este momento no hay citas disponibles",
            "no hay citas disponibles",
            "no existen citas disponibles",
        ],
    );
    let captcha_text = has_any(&text, &["captcha", "código de la imagen"]);
    // Python _challenge_code returns CAPTCHA before HTTP_BLOCKED when both are present.
    let blocked = !captcha_text && block_marker(&text).is_some();
    let booking = [
        "input[name*='fecha']",
        "select[name*='fecha']",
        "input[name*='hora']",
        "select[name*='hora']",
        ".ui-datepicker",
        "input[type='radio'][name*='cita' i]",
    ]
    .iter()
    .any(|s| visible(s));
    let captcha = captcha_text
        || [
            "#captcha",
            "input[name*='captcha' i]",
            "img[src*='captcha' i]",
        ]
        .iter()
        .any(|s| visible(s));
    let applicant = visible("#txtIdCitado")
        && (snapshot.applicant_editable
            || (snapshot.fill_mode == "copy" && visible("#btnCopiar") && visible("#btnEnviar")));
    result.kind = if blocked {
        PageKind::Blocked
    } else if !no_slots && booking {
        PageKind::Booking
    } else if captcha {
        PageKind::Captcha
    } else if visible("#form") {
        PageKind::Province
    } else if visible("select[id^=\"tramiteGrupo[\"]:visible") {
        PageKind::Procedure
    } else if visible("#btnAccesoClave") {
        PageKind::Auth
    } else if applicant {
        if no_slots {
            PageKind::ApplicantNoSlots
        } else {
            PageKind::Applicant
        }
    } else if no_slots {
        if snapshot
            .pending_action
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(snapshot.last_action.as_deref())
            == Some("contact")
        {
            PageKind::PostContactNoSlots
        } else if visible("form[action=\"salirInicio\"] #btnSubmit")
            && normalize(&snapshot.button_label) == "aceptar"
        {
            PageKind::RequestNoSlotsRestart
        } else {
            PageKind::RequestNoSlots
        }
    } else if matches!(
        snapshot.endpoint.as_str(),
        "acValidarEntrada" | "acOpcDirect"
    ) && visible("#btnEnviar")
    {
        if snapshot.button_label.to_lowercase().contains("solicitar") {
            PageKind::RequestMenu
        } else {
            PageKind::Unknown
        }
    } else if visible("#idSede") || visible("select[name='idSede']") {
        PageKind::Office
    } else if ["#txtTelefonoCitado", "#emailUNO", "#emailDOS"]
        .iter()
        .all(|s| visible(s))
    {
        PageKind::Contact
    } else if !snapshot.official {
        PageKind::AuthWait
    } else {
        PageKind::Unknown
    };
    if result.kind == PageKind::Unknown
        && matches!(
            text.as_str(),
            "" | "proceso automático para la solicitud de cita previa"
                | "proceso automatico para la solicitud de cita previa"
        )
    {
        result.kind = PageKind::Loading;
    }
    if result.kind == PageKind::Unknown {
        result.variant = shape_hash(&snapshot.shape);
    }
    if matches!(
        result.kind,
        PageKind::ApplicantNoSlots
            | PageKind::RequestNoSlots
            | PageKind::RequestNoSlotsRestart
            | PageKind::PostContactNoSlots
    ) {
        result.operation_code = has_any(&text, &["cod. oper", "cod.oper", "cod oper", "codoper"]);
    }
    Ok(result)
}
