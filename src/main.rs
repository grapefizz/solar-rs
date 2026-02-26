use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::sleep;
use url::Url;

#[derive(Debug, Clone, Copy)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Debug, Clone)]
struct BodyState {
    name: &'static str,
    id: &'static str,
    pos_au: Option<Vec3>,
}

#[derive(Debug, Clone)]
struct AppState {
    bodies: Vec<BodyState>,
    last_update_utc: Option<String>,
    status: String,
    use_unicode_icons: bool,

    // Zoom controls
    zoom: f64,          // multiplicative zoom factor (1.0 default)
    focus_index: usize, // which max-orbit target we fit to
}

#[derive(Debug, Deserialize)]
struct HorizonsJson {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    result: String,
}

#[derive(Debug, Clone, Copy)]
struct BodyMeta {
    name: &'static str,
    id: &'static str,
    nf_icon: char,
    uni_icon: char,
    color: Color,
    orbit_au: Option<f64>,
}

const BODIES: &[BodyMeta] = &[
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

// Which orbit radius we “fit to” (max visible orbit)
const FOCUS_LEVELS: &[(&str, f64)] = &[
    ("Earth",   1.0),
    ("Mars",    1.523679),
    ("Jupiter", 5.203800),
    ("Saturn",  9.537070),
    ("Uranus",  19.19126),
    ("Neptune", 30.06896),
];

fn meta_by_name(name: &str) -> Option<BodyMeta> {
    BODIES.iter().copied().find(|m| m.name == name)
}

fn icon_for(meta: BodyMeta, use_unicode: bool) -> char {
    if use_unicode { meta.uni_icon } else { meta.nf_icon }
}

fn build_horizons_url(body_id: &str, start_utc: &str, stop_utc: &str) -> Result<Url> {
    let mut url = Url::parse("https://ssd.jpl.nasa.gov/api/horizons.api")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("format", "json");
        qp.append_pair("MAKE_EPHEM", "YES");
        qp.append_pair("OBJ_DATA", "NO");
        qp.append_pair("EPHEM_TYPE", "VECTORS");

        qp.append_pair("COMMAND", body_id);
        qp.append_pair("CENTER", "500@10");
        qp.append_pair("REF_PLANE", "ECLIPTIC");
        qp.append_pair("REF_SYSTEM", "ICRF");
        qp.append_pair("OUT_UNITS", "AU-D");
        qp.append_pair("CSV_FORMAT", "YES");
        qp.append_pair("VEC_TABLE", "1");
        qp.append_pair("TIME_TYPE", "UT");

        qp.append_pair("START_TIME", &format!("'{}'", start_utc));
        qp.append_pair("STOP_TIME", &format!("'{}'", stop_utc));
        qp.append_pair("STEP_SIZE", "'1 m'");
    }
    Ok(url)
}

fn extract_table_lines(result_text: &str) -> Result<Vec<&str>> {
    let so = result_text.find("$$SOE").ok_or_else(|| anyhow!("Missing $$SOE marker"))?;
    let eo = result_text.find("$$EOE").ok_or_else(|| anyhow!("Missing $$EOE marker"))?;
    if eo <= so {
        return Err(anyhow!("$$EOE occurs before $$SOE"));
    }
    let table = &result_text[(so + 5)..eo];
    Ok(table.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect())
}

fn parse_xyz_from_csv_row(row: &str) -> Result<Vec3> {
    let cols: Vec<&str> = row
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if cols.len() < 5 {
        return Err(anyhow!("Unexpected CSV format: {}", row));
    }

    let x = cols[cols.len() - 3].parse::<f64>().context("parse x")?;
    let y = cols[cols.len() - 2].parse::<f64>().context("parse y")?;
    let z = cols[cols.len() - 1].parse::<f64>().context("parse z")?;
    Ok(Vec3 { x, y, z })
}

async fn fetch_body_vec(client: &reqwest::Client, body_id: &str, start_utc: &str, stop_utc: &str) -> Result<Vec3> {
    let url = build_horizons_url(body_id, start_utc, stop_utc)?;
    let body = client.get(url).send().await?.error_for_status()?.text().await?;
    let parsed: HorizonsJson = serde_json::from_str(&body).context("parse Horizons JSON")?;
    if let Some(e) = parsed.error {
        return Err(anyhow!("Horizons error: {}", e));
    }
    let lines = extract_table_lines(&parsed.result)?;
    for line in lines {
        if let Ok(v) = parse_xyz_from_csv_row(line) {
            return Ok(v);
        }
    }
    Err(anyhow!("No parseable vector row for body {}", body_id))
}

async fn updater(state: Arc<Mutex<AppState>>) {
    let client = reqwest::Client::builder()
        .user_agent("solar-rs/0.5 (ratatui)")
        .build()
        .expect("reqwest client");

    loop {
        let now_label = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

        let start = Utc::now();
        let stop = start + ChronoDuration::minutes(1);
        let start_str = start.format("%Y-%b-%d %H:%M:%S").to_string();
        let stop_str = stop.format("%Y-%b-%d %H:%M:%S").to_string();

        let bodies_snapshot = {
            let s = state.lock().unwrap();
            s.bodies.iter().filter(|b| b.id != "10").map(|b| (b.name, b.id)).collect::<Vec<_>>()
        };

        let mut new_positions: BTreeMap<&'static str, Vec3> = BTreeMap::new();
        let mut status = "OK".to_string();

        for (name, id) in bodies_snapshot {
            match fetch_body_vec(&client, id, &start_str, &stop_str).await {
                Ok(v) => { new_positions.insert(name, v); }
                Err(e) => status = format!("Fetch error ({}): {}", name, e),
            }
            sleep(Duration::from_millis(120)).await;
        }

        {
            let mut s = state.lock().unwrap();
            for b in &mut s.bodies {
                if b.id == "10" {
                    b.pos_au = Some(Vec3 { x: 0.0, y: 0.0, z: 0.0 });
                } else {
                    b.pos_au = new_positions.get(b.name).copied().or(b.pos_au);
                }
            }
            s.last_update_utc = Some(now_label);
            s.status = status;
        }

        sleep(Duration::from_secs(5)).await;
    }
}

fn draw_ui(f: &mut Frame, state: &AppState) {
    let (focus_name, focus_au) = FOCUS_LEVELS[state.focus_index];

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    let header = Paragraph::new(Line::from(format!(
        "Last update: {} | Status: {} | zoom: {:.2}x | focus: {} ({:.2} AU) | +/- zoom, 0 reset, [ ] focus, q quit",
        state.last_update_utc.as_deref().unwrap_or("—"),
        state.status,
        state.zoom,
        focus_name,
        focus_au
    )))
    .block(Block::default().borders(Borders::ALL).title("Solar System"));

    f.render_widget(header, root[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(root[1]);

    // Table
    let rows = state.bodies.iter().map(|b| {
        let icon_cell = if let Some(m) = meta_by_name(b.name) {
            Cell::from(Span::styled(
                icon_for(m, state.use_unicode_icons).to_string(),
                Style::default().fg(m.color),
            ))
        } else {
            Cell::from("?")
        };

        let (x, y, z, r) = if let Some(v) = b.pos_au {
            let r = (v.x * v.x + v.y * v.y).sqrt();
            (
                format!("{:+.6}", v.x),
                format!("{:+.6}", v.y),
                format!("{:+.6}", v.z),
                format!("{:.6}", r),
            )
        } else {
            ("—".into(), "—".into(), "—".into(), "—".into())
        };

        Row::new(vec![
            icon_cell,
            Cell::from(b.name),
            Cell::from(x),
            Cell::from(y),
            Cell::from(z),
            Cell::from(r),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(vec!["", "Body", "X", "Y", "Z", "R"]).style(Style::default()))
    .block(Block::default().borders(Borders::ALL).title("Heliocentric vectors (AU)"));

    f.render_widget(table, main[0]);

    // Map
    let map = render_map_block(main[1], state);
    f.render_widget(map, main[1]);
}

#[derive(Clone, Copy)]
struct Pixel { ch: char, color: Color, priority: u8 }

fn put_pixel(grid: &mut [Vec<Option<Pixel>>], x: i32, y: i32, p: Pixel) {
    if x < 0 || y < 0 { return; }
    let (yu, xu) = (y as usize, x as usize);
    if yu >= grid.len() || xu >= grid[0].len() { return; }
    match grid[yu][xu] {
        None => grid[yu][xu] = Some(p),
        Some(existing) if p.priority > existing.priority => grid[yu][xu] = Some(p),
        _ => {}
    }
}

fn draw_ring(grid: &mut [Vec<Option<Pixel>>], cx: i32, cy: i32, r_pix: f64) {
    if r_pix < 1.0 { return; }
    let steps = (r_pix * 6.0).clamp(64.0, 720.0) as i32;
    for i in 0..steps {
        let t = (i as f64) * std::f64::consts::TAU / (steps as f64);
        let x = cx + (t.cos() * r_pix).round() as i32;
        let y = cy - (t.sin() * r_pix).round() as i32;
        put_pixel(grid, x, y, Pixel { ch: '·', color: Color::DarkGray, priority: 1 });
    }
}

fn render_map_block(area: Rect, state: &AppState) -> Paragraph<'static> {
    let w = area.width.saturating_sub(2) as usize;
    let h = area.height.saturating_sub(2) as usize;
    let w = w.max(1);
    let h = h.max(1);

    let mut grid: Vec<Vec<Option<Pixel>>> = vec![vec![None; w]; h];
    let cx = (w / 2) as i32;
    let cy = (h / 2) as i32;

    // Base scale: fit selected focus orbit to the panel
    let (_, focus_au) = FOCUS_LEVELS[state.focus_index];
    let base_scale = (w.min(h) as f64 * 0.45) / focus_au.max(0.1);
    let scale = base_scale * state.zoom;

    // Orbit rings up to focus orbit (so zoom/focus actually changes what you see)
    for m in BODIES {
        if let Some(r_au) = m.orbit_au {
            if r_au <= focus_au {
                draw_ring(&mut grid, cx, cy, r_au * scale);
            }
        }
    }

    // Sun
    if let Some(sun) = meta_by_name("Sun") {
        put_pixel(&mut grid, cx, cy, Pixel {
            ch: icon_for(sun, state.use_unicode_icons),
            color: sun.color,
            priority: 10,
        });
    }

    // Planets
    for b in &state.bodies {
        let Some(v) = b.pos_au else { continue };
        let Some(m) = meta_by_name(b.name) else { continue };

        // If we're focused in (say Jupiter), still draw outer planets if they fall inside view
        // BUT their orbit rings may not be drawn. That's ok.
        let sx = (v.x * scale).round() as i32;
        let sy = (v.y * scale).round() as i32;
        let x = cx + sx;
        let y = cy - sy;

        put_pixel(&mut grid, x, y, Pixel {
            ch: icon_for(m, state.use_unicode_icons),
            color: m.color,
            priority: 20,
        });
    }

    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for row in grid {
        let mut spans = Vec::with_capacity(w);
        for cell in row {
            match cell {
                Some(p) => spans.push(Span::styled(p.ch.to_string(), Style::default().fg(p.color))),
                None => spans.push(Span::raw(" ")),
            }
        }
        lines.push(Line::from(spans));
    }

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Orbits + positions"))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn has_arg(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn clamp_zoom(z: f64) -> f64 {
    z.clamp(0.2, 50.0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let use_unicode_icons = has_arg("--unicode");

    let bodies = BODIES
        .iter()
        .map(|m| BodyState { name: m.name, id: m.id, pos_au: None })
        .collect::<Vec<_>>();

    let state = Arc::new(Mutex::new(AppState {
        bodies,
        last_update_utc: None,
        status: "Starting…".into(),
        use_unicode_icons,
        zoom: 1.0,
        focus_index: FOCUS_LEVELS.len() - 1, // default: Neptune fit
    }));

    tokio::spawn(updater(state.clone()));

    let mut terminal = setup_terminal()?;

    loop {
        let snapshot = { state.lock().unwrap().clone() };
        terminal.draw(|f| draw_ui(f, &snapshot))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break,

                    // zoom in
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = clamp_zoom(s.zoom * 1.25);
                    }
                    // zoom out
                    KeyCode::Char('-') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = clamp_zoom(s.zoom / 1.25);
                    }
                    // reset zoom
                    KeyCode::Char('0') => {
                        let mut s = state.lock().unwrap();
                        s.zoom = 1.0;
                        s.focus_index = FOCUS_LEVELS.len() - 1;
                    }
                    // focus in reminder: smaller max orbit
                    KeyCode::Char('[') => {
                        let mut s = state.lock().unwrap();
                        if s.focus_index > 0 {
                            s.focus_index -= 1;
                        }
                    }
                    // focus out: larger max orbit
                    KeyCode::Char(']') => {
                        let mut s = state.lock().unwrap();
                        if s.focus_index + 1 < FOCUS_LEVELS.len() {
                            s.focus_index += 1;
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    restore_terminal(terminal)?;
    Ok(())
}
