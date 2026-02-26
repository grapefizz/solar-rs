use ratatui::style::Color;
use serde::Deserialize;

#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone)]
pub struct BodyState {
    pub name: &'static str,
    pub id: &'static str,
    pub pos_au: Option<Vec3>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub bodies: Vec<BodyState>,
    pub last_update_utc: Option<String>,
    pub status: String,
    pub use_unicode_icons: bool,

    // Zoom controls
    pub zoom: f64,          // multiplicative zoom factor (1.0 default)
    pub focus_index: usize, // which max-orbit target we fit to
}

#[derive(Debug, Deserialize)]
pub struct HorizonsJson {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub result: String,
}

#[derive(Debug, Clone, Copy)]
pub struct BodyMeta {
    pub name: &'static str,
    pub id: &'static str,
    pub nf_icon: char,
    pub uni_icon: char,
    pub color: Color,
    pub orbit_au: Option<f64>,
}

pub const BODIES: &[BodyMeta] = &[
    BodyMeta { name: "Sun",     id: "10",  nf_icon: '\u{F185}', uni_icon: '', color: Color::Yellow,orbit_au: None },
    BodyMeta { name: "Mercury", id: "199", nf_icon: '', uni_icon: '', color: Color::LightMagenta, orbit_au: Some(0.387098) },
    BodyMeta { name: "Venus",   id: "299", nf_icon: '', uni_icon: '', color: Color::LightYellow,  orbit_au: Some(0.723332) },
    BodyMeta { name: "Earth",   id: "399", nf_icon: '', uni_icon: '', color: Color::LightBlue,    orbit_au: Some(1.000000) },
    BodyMeta { name: "Mars",    id: "499", nf_icon: '', uni_icon: '', color: Color::Red,          orbit_au: Some(1.523679) },
    BodyMeta { name: "Jupiter", id: "599", nf_icon: '', uni_icon: '', color: Color::LightRed,     orbit_au: Some(5.203800) },
    BodyMeta { name: "Saturn",  id: "699", nf_icon: '', uni_icon: '', color: Color::LightYellow,  orbit_au: Some(9.537070) },
    BodyMeta { name: "Uranus",  id: "799", nf_icon: '', uni_icon: '', color: Color::Cyan,         orbit_au: Some(19.19126) },
    BodyMeta { name: "Neptune", id: "899", nf_icon: '', uni_icon: '', color: Color::Blue,         orbit_au: Some(30.06896) },
];

pub const FOCUS_LEVELS: &[(&str, f64)] = &[
    ("Earth",   1.0),
    ("Mars",    1.523679),
    ("Jupiter", 5.203800),
    ("Saturn",  9.537070),
    ("Uranus",  19.19126),
    ("Neptune", 30.06896),
];

pub fn meta_by_name(name: &str) -> Option<BodyMeta> {
    BODIES.iter().copied().find(|m| m.name == name)
}

pub fn icon_for(meta: BodyMeta, use_unicode: bool) -> char {
    if use_unicode { meta.uni_icon } else { meta.nf_icon }
}
