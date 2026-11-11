//! Layout of the public achievement card (Python 0.3.6 `achievement_card.py`) as drawing operations.
//! The UI replays them on a canvas; only allowlisted statistics reach it, never appointment details.
use super::{duration, public_stats};
use serde_json::{Value, json};

const WIDTH: i64 = 1200;
const HEIGHT: i64 = 1240;
/// Advance widths of the 24 px semibold value font, measured from the bundled Plex face (Python fixture).
const DIGIT_ADVANCE: f64 = 14.390625;
const DASH_ADVANCE: f64 = 18.71875;
const POINT_ADVANCE: f64 = 7.140625;

struct Theme {
    background: &'static str,
    surface: &'static str,
    line: &'static str,
    success: &'static str,
    text: &'static str,
    muted: &'static str,
    accent: &'static str,
    gold: &'static str,
}
const DARK: Theme = Theme {
    background: "#1d1e22",
    surface: "#26272c",
    line: "#3a3b40",
    success: "#6bcf8e",
    text: "#e8e8e8",
    muted: "#9a9b9f",
    accent: "#8a93f5",
    gold: "#e9b44c",
};
const LIGHT: Theme = Theme {
    background: "#ffffff",
    surface: "#f7f6f5",
    line: "#e6e6e6",
    success: "#1b7a5a",
    text: "#282a30",
    muted: "#6f6f6f",
    accent: "#5e6ad2",
    gold: "#a56c00",
};

/// (Russian source, English, Spanish) of every card string.
const TEXTS: [(&str, &str, &str); 24] = [
    ("Сита получена", "Appointment booked", "Cita conseguida"),
    ("Поиск завершён", "Search ended", "Búsqueda finalizada"),
    ("Проверок сит", "Appointment checks", "Comprobaciones de citas"),
    ("Активный поиск", "Active search", "Búsqueda activa"),
    ("Дни с поиском", "Days with searches", "Días de búsqueda"),
    ("Доступность", "Availability", "Disponibilidad"),
    ("Появлений офисов", "Office appearances", "Apariciones de oficinas"),
    ("Разных офисов", "Unique offices", "Oficinas distintas"),
    ("Появления выбора даты", "Date selection appearances", "Apariciones de selección de fecha"),
    ("«Сит нет» до запроса", "“No appointments” before requesting", "«No hay citas» antes de solicitar"),
    ("Ответы «Сит нет» на запрос записи", "No-appointments replies to booking requests", "Respuestas sin citas a solicitudes de cita"),
    ("Ошибки и восстановление", "Errors and recovery", "Errores y recuperación"),
    ("Ошибок сервера", "Server errors", "Errores del servidor"),
    ("Ограничений HTTP 429", "HTTP 429 limits", "Límites HTTP 429"),
    ("Ограничения доступа", "Access restrictions", "Restricciones de acceso"),
    ("Повторные проходы", "Restarts from the beginning", "Reinicios desde el principio"),
    ("Ответы «Сит нет» с cod. oper", "No-appointments replies with cod. oper", "Respuestas sin citas con cod. oper"),
    ("Автоматизация", "Automation", "Automatización"),
    ("Действий выполнено", "Actions performed", "Acciones realizadas"),
    ("Страниц обработано", "Pages processed", "Páginas procesadas"),
    ("Первый офис", "First office", "Primera oficina"),
    ("Первый выбор даты", "First date selection", "Primera selección de fecha"),
    ("{duration} поиска", "{duration} of search", "{duration} de búsqueda"),
    ("с", "s", "s"),
];

struct Card {
    language: String,
    theme: &'static Theme,
    ops: Vec<Value>,
}

/// An integer-valued number stays an integer; Python only produced floats where it computed one.
fn number(value: f64, integral: bool) -> Value {
    if integral { json!(value as i64) } else { json!(value) }
}

impl Card {
    fn t(&self, source: &str) -> String {
        TEXTS
            .iter()
            .find(|row| row.0 == source)
            .map(|row| match self.language.as_str() {
                "ru" => row.0,
                "es" => row.2,
                _ => row.1,
            })
            .unwrap_or(source)
            .to_string()
    }
    fn rectangle(&mut self, x: i64, y: i64, w: Value, h: i64, color: &str, border: Option<&str>, radius: i64) {
        self.ops.push(json!({"op":"box","x":x,"y":y,"w":w,"h":h,"color":color,"border":border,"radius":radius}));
    }
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, x: Value, y: i64, w: Value, h: i64, value: &str, size: i64, color: Option<&str>, bold: bool, right: bool, wrap: bool) {
        let color = color.unwrap_or(self.theme.text);
        self.ops.push(json!({"op":"text","x":x,"y":y,"w":w,"h":h,"value":value,"size":size,"color":color,"bold":bold,"right":right,"wrap":wrap}));
    }
    fn line(&mut self, x: i64, y: i64, w: i64) {
        self.ops.push(json!({"op":"line","x":x,"y":y,"w":w,"color":self.theme.line}));
    }
    fn icon(&mut self, name: &str, x: i64, y: i64, size: i64, color: Option<&str>) {
        let color = color.unwrap_or(self.theme.text);
        self.ops.push(json!({"op":"icon","name":name,"x":x,"y":y,"size":size,"color":color}));
    }
    fn hero(&mut self) {
        let theme = self.theme;
        self.rectangle(0, 0, json!(1200), 6, theme.success, None, 0);
        self.rectangle(48, 40, json!(1104), 220, theme.surface, Some(theme.line), 18);
        let pieces: [(i64, i64, i64, i64, i64, &str); 10] = [
            (85, 74, 14, 5, -25, theme.success), (175, 96, 6, 12, 23, theme.accent),
            (299, 70, 12, 5, 40, theme.gold), (416, 102, 5, 11, -28, theme.success),
            (557, 72, 13, 5, -18, theme.accent), (710, 101, 6, 12, 28, theme.gold),
            (856, 70, 12, 5, 35, theme.success), (940, 221, 12, 5, -30, theme.accent),
            (1115, 75, 6, 13, 26, theme.gold), (1120, 218, 12, 5, -35, theme.success),
        ];
        for (x, y, w, h, angle, color) in pieces {
            self.ops.push(json!({"op":"save"}));
            self.ops.push(json!({"op":"translate","x":x,"y":y}));
            self.ops.push(json!({"op":"rotate","angle":angle}));
            self.rectangle(0, 0, json!(w), h, color, None, 1);
            self.ops.push(json!({"op":"restore"}));
        }
        for (x, y, size, color) in [(916, 121, 8, theme.gold), (1099, 174, 7, theme.accent), (1012, 69, 6, theme.success)] {
            let (x, y, size) = (x as f64, y as f64, size as f64);
            self.ops.push(json!({"op":"stroke","color":color,"width":2,"x1":x - size,"y1":y,"x2":x + size,"y2":y}));
            self.ops.push(json!({"op":"stroke","color":color,"width":2,"x1":x,"y1":y - size,"x2":x,"y2":y + size}));
        }
        let title = self.t("Сита получена") + "!";
        self.text(json!(80), 122, json!(850), 80, &title, 64, None, true, false, false);
        let subtitle = self.t("Поиск завершён");
        self.text(json!(83), 202, json!(850), 36, &subtitle, 25, Some(theme.muted), false, false, false);
        self.icon("calendar", 978, 90, 120, Some(theme.success));
    }
    fn tile(&mut self, x: i64, value: &str, title: &str, icon: &str) {
        let theme = self.theme;
        self.rectangle(x, 292, json!(352), 142, theme.surface, Some(theme.line), 10);
        self.text(json!(x + 24), 307, json!(304), 68, value, 45, None, true, false, false);
        self.icon(icon, x + 24, 388, 24, Some(theme.muted));
        let title = self.t(title);
        self.text(json!(x + 60), 379, json!(274), 40, &title, 22, Some(theme.muted), false, false, true);
    }
    fn group(&mut self, title: &str, icon: &str, x: i64, items: &[(&str, &str, String)]) {
        let theme = self.theme;
        self.icon(icon, x, 480, 28, None);
        let title = self.t(title);
        self.text(json!(x + 42), 476, json!(484), 36, &title, 25, None, true, false, false);
        self.line(x, 528, 526);
        for (index, (glyph, label, value)) in items.iter().enumerate() {
            let y = 538 + index as i64 * 54;
            let advance: f64 = value
                .chars()
                .map(|c| match c {
                    '—' => DASH_ADVANCE,
                    '.' => POINT_ADVANCE,
                    _ => DIGIT_ADVANCE,
                })
                .sum();
            // Python: max(80, min(190, advance + 6)) — an integer only where a bound was returned.
            let wanted = advance + 6.0;
            let (width, integral) = if wanted <= 80.0 { (80.0, true) } else if wanted >= 190.0 { (190.0, true) } else { (wanted, false) };
            self.icon(glyph, x, y + 13, 24, Some(theme.muted));
            let label = self.t(label);
            self.text(json!(x + 38), y, number(526.0 - 54.0 - width, integral), 52, &label, 22, None, false, false, true);
            self.text(number((x + 526) as f64 - width, integral), y, number(width, integral), 52, value, 24, None, true, true, false);
            if index + 1 < items.len() {
                self.line(x, y + 51, 526);
            }
        }
    }
}

pub fn card_layout(achievement: &Value, language: &str, dark: bool) -> Value {
    let stats = public_stats(achievement);
    let theme: &'static Theme = if dark { &DARK } else { &LIGHT };
    let mut card = Card { language: language.to_string(), theme, ops: Vec::new() };
    let value = |key: &str| match &stats[key] {
        Value::Null => "—".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let seconds_text = |card: &Card, key: &str| -> String {
        match stats[key].as_f64() {
            None => "—".to_string(),
            Some(seconds) if seconds < 60.0 => format!("{} {}", seconds as i64, card.t("с")),
            Some(_) => duration(&stats[key], &card.language),
        }
    };
    card.hero();
    card.tile(48, &value("checks"), "Проверок сит", "search");
    let active = seconds_text(&card, "active_seconds");
    card.tile(424, &active, "Активный поиск", "clock");
    card.tile(800, &value("days"), "Дни с поиском", "calendar_days");
    card.group("Доступность", "eye", 48, &[
        ("office", "Появлений офисов", value("offices_seen")),
        ("pin", "Разных офисов", value("unique_offices")),
        ("calendar_first", "Появления выбора даты", value("booking_screens")),
        ("clipboard", "«Сит нет» до запроса", value("banners")),
        ("message", "Ответы «Сит нет» на запрос записи", value("no_slots")),
    ]);
    card.group("Ошибки и восстановление", "wrench", 626, &[
        ("server", "Ошибок сервера", value("server_errors")),
        ("hourglass", "Ограничений HTTP 429", value("rate_limited")),
        ("lock", "Ограничения доступа", value("blocked")),
        ("restart", "Повторные проходы", value("flow_passes")),
        ("shield-alert", "Ответы «Сит нет» с cod. oper", value("cod_oper_count")),
    ]);
    card.icon("settings", 48, 838, 28, None);
    let automation = card.t("Автоматизация");
    card.text(json!(90), 831, json!(1062), 40, &automation, 25, None, true, false, false);
    card.line(48, 881, 1104);
    for (x, key, label, icon) in [(48, "actions_performed", "Действий выполнено", "pointer"), (626, "pages_processed", "Страниц обработано", "file_text")] {
        card.icon(icon, x, 908, 26, Some(theme.muted));
        card.text(json!(x + 40), 896, json!(486), 48, &value(key), 32, None, true, false, false);
        let label = card.t(label);
        card.text(json!(x), 946, json!(526), 36, &label, 22, Some(theme.muted), false, false, false);
    }
    card.line(48, 1003, 1104);
    for (x, prefix, icon, label) in [(48, "first_office", "clock", "Первый офис"), (626, "first_booking", "calendar", "Первый выбор даты")] {
        let mut amount = seconds_text(&card, &format!("{prefix}_seconds"));
        if amount != "—" {
            if stats[format!("{prefix}_active_quality")] == "minimum" {
                amount = format!("≥ {amount}");
            }
            amount = card.t("{duration} поиска").replace("{duration}", &amount);
        }
        card.icon(icon, x, 1031, 26, Some(theme.muted));
        let label = card.t(label);
        card.text(json!(x + 40), 1017, json!(486), 40, &label, 22, Some(theme.muted), false, false, false);
        card.text(json!(x + 40), 1060, json!(486), 40, &amount, 27, None, true, false, false);
    }
    card.line(48, 1144, 1104);
    card.icon("activity", 48, 1172, 28, Some(theme.accent));
    card.text(json!(92), 1164, json!(1000), 42, "SantaCita", 25, None, true, false, false);
    card.text(json!(92), 1203, json!(1000), 28, "github.com/itwormz/extranjeria-cita-helper", 19, Some(theme.muted), false, false, false);
    json!({"width":WIDTH,"height":HEIGHT,"background":theme.background,"ops":card.ops})
}
