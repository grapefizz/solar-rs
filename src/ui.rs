use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::types::{icon_for, meta_by_name, AppState, BODIES, FOCUS_LEVELS};

pub fn draw_ui(f: &mut Frame, state: &AppState) {
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
