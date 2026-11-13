//! Draws the app on the round face.
//!
//! Wheels (the Controls list, a combo box's choices) are drawn as a drum:
//! when the choice moves, the rows roll to their new places, shrinking and
//! fading as they turn away from the middle. A [`Renderer`] keeps where the
//! drum has got to between frames; [`render`] draws one still frame.

use qsys::{Control, Widget};
use serde_json::Value;

use std::cell::Cell;

use crate::app::{App, Item, Layout, Mode, Screen, Settings, SettingsRow, SETTINGS, SETTINGS_ROWS};
use crate::canvas::{mix, Canvas, Weight, FACE};

const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const DIM: u32 = 0x9a9a9a;
const DIM2: u32 = 0x4e4e4e;
const TRACK: u32 = 0x2c2c2c;
const ACCENT: u32 = 0x2fa8ff;
const EDIT: u32 = 0xff9a2e;
const RED: u32 = 0xff453a;

const C: f32 = FACE as f32 / 2.0;
/// The edge arc of dials and the volume: its radius and width.
const EDGE_R: f32 = 212.0;
const EDGE_W: f32 = 26.0;
/// It starts at the lower left and runs clockwise to the lower right.
const EDGE_START: f32 = 135.0;
const EDGE_SWEEP: f32 = 270.0;

/// The value as shown: the Core's String, else the value itself.
fn shown(c: &Control) -> String {
    if let Some(s) = c.string.as_deref().filter(|s| !s.is_empty()) {
        return s.to_owned();
    }
    match &c.value {
        Value::Bool(b) => if *b { "On" } else { "Off" }.into(),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            let s = format!("{f:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// How quickly a drum settles: the time for it to cover 63% of the way.
const DRUM_TAU_S: f32 = 0.075;
/// Each row of a drum is this many degrees around it from the next.
const DRUM_STEP_DEG: f32 = 25.0;
/// The drum's radius: how far from the middle rows sit at a quarter turn.
const DRUM_R: f32 = 190.0;

/// Where a drum has rolled to, in rows (counting on past the last row
/// rather than wrapping, so a wrap rolls the short way).
struct Drum {
    key: String,
    n: usize,
    index: usize,
    target: f32,
    pos: f32,
}

/// What a frame needs from the frames before it.
struct Anim {
    drum: Drum,
    dt: f32,
    moving: bool,
}

impl Anim {
    /// The drum's position for wheel `key` of n rows, now on row `index`.
    fn drum(&mut self, key: &str, n: usize, index: usize) -> f32 {
        let d = &mut self.drum;
        if d.key != key || d.n != n {
            *d = Drum { key: key.to_owned(), n, index, target: index as f32, pos: index as f32 };
        } else if index != d.index {
            let n = n as i64;
            let mut step = (index as i64 - d.index as i64).rem_euclid(n);
            if step > n / 2 {
                step -= n;
            }
            d.target += step as f32;
            d.index = index;
        }
        d.pos += (d.target - d.pos) * (1.0 - (-self.dt / DRUM_TAU_S).exp());
        if (d.target - d.pos).abs() < 0.002 {
            d.pos = d.target;
        } else {
            self.moving = true;
        }
        d.pos
    }
}

/// Draws frames that follow on from each other, for animation.
pub struct Renderer {
    anim: Anim,
    last_ms: Option<u64>,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        let drum = Drum { key: String::new(), n: 0, index: 0, target: 0.0, pos: 0.0 };
        Renderer { anim: Anim { drum, dt: 0.0, moving: false }, last_ms: None }
    }

    /// Draws the app as at now_ms; returns whether something is still
    /// moving (so another frame is wanted soon).
    pub fn frame(&mut self, app: &App, canvas: &mut Canvas, now_ms: u64) -> bool {
        let dt = self.last_ms.map_or(0.0, |t| now_ms.saturating_sub(t) as f32 / 1000.0);
        self.last_ms = Some(now_ms);
        self.anim.dt = dt.min(0.1);
        self.anim.moving = false;
        draw(app, canvas, &mut self.anim);
        self.anim.moving
    }
}

/// One still frame, every drum at rest.
pub fn render(app: &App, canvas: &mut Canvas) {
    Renderer::new().frame(app, canvas, 0);
}

fn draw(app: &App, canvas: &mut Canvas, anim: &mut Anim) {
    HINTS.with(|h| h.set(app.settings.hints));
    canvas.clear(BG);
    if app.idle {
        home(app, canvas);
        canvas.mask_round();
        return;
    }
    match (app.current(), app.mode) {
        (_, Mode::Menu { focus }) => menu(app, focus, canvas, anim),
        (_, Mode::Settings { focus }) => settings(app, focus, canvas, anim),
        (_, Mode::Network) => network(app, canvas),
        (None, _) => {
            status(canvas, "No controls", "Give components a NEST_ Code Name and Script Access External");
            hint(canvas, 360.0, "Hold for Settings");
        }
        (Some(screen), _) if matches!(screen.layout, Layout::Volume { .. }) => volume(screen, canvas),
        (Some(screen), mode) if matches!(screen.layout, Layout::Selector { .. }) => selector(screen, mode, canvas, anim),
        (Some(screen), mode) => {
            match app.focused() {
                Some(item) => control(app, screen, item, mode, canvas, anim),
                None => controls_stop(screen, canvas),
            }
            // A picker's wheel of choices has the bottom to itself.
            if !matches!((mode, app.focused().map(|i| &i.widget)), (Mode::Edit { .. }, Some(Widget::Picker { .. }))) {
                let stops = screen.stops();
                dots(stops.iter().position(|&i| i == app.focus).unwrap_or(0), stops.len(), canvas);
            }
        }
    }
    canvas.mask_round();
}

/// A whole-face message (no controls, connecting, ...).
pub fn status(canvas: &mut Canvas, title: &str, detail: &str) {
    canvas.clear(BG);
    canvas.text_centred(C, 200.0, title, 40.0, Weight::Bold, 380.0, FG);
    let lines = canvas.fonts.wrap(detail, 22.0, Weight::Regular, 320.0, 3);
    for (i, line) in lines.iter().enumerate() {
        canvas.text_centred(C, 262.0 + 32.0 * i as f32, line, 22.0, Weight::Regular, 340.0, DIM);
    }
    canvas.mask_round();
}

fn header(canvas: &mut Canvas, title: &str, label: &str) {
    canvas.text_centred(C, 80.0, title, 22.0, Weight::Regular, 260.0, DIM);
    canvas.text_centred(C, 128.0, label, 38.0, Weight::Bold, 360.0, FG);
}

thread_local! {
    /// Whether this frame shows hints: the app's Settings, set as each frame
    /// starts, so every drawing function need not carry them.
    static HINTS: Cell<bool> = const { Cell::new(true) };
}

/// A small "Click to ..." hint, unless Settings turned them off.
fn hint(canvas: &mut Canvas, y: f32, text: &str) {
    hint_sized(canvas, y, text, 19.0);
}

fn hint_sized(canvas: &mut Canvas, y: f32, text: &str, size: f32) {
    if HINTS.with(Cell::get) {
        canvas.text_centred(C, y, text, size, Weight::Regular, 300.0, DIM);
    }
}

/// The list of components, as a wheel of names around the one shown.
fn menu(app: &App, focus: usize, canvas: &mut Canvas, anim: &mut Anim) {
    canvas.text_centred(C, 58.0, "Controls", 20.0, Weight::Regular, 200.0, DIM);
    let entries = app.menu_entries();
    let at = entries.iter().position(|&i| i == focus).unwrap_or(0);
    let titles: Vec<String> = entries
        .iter()
        .map(|&i| if i == SETTINGS { "Settings".to_owned() } else { app.screens[i].title.clone() })
        .collect();
    let pos = anim.drum("menu", titles.len(), at);
    drum(canvas, &titles, pos, TRACK, FG);
    dots(at, titles.len(), canvas);
}

/// The Settings list, as a drum of its rows.
fn settings(app: &App, focus: usize, canvas: &mut Canvas, anim: &mut Anim) {
    canvas.text_centred(C, 58.0, "Settings", 20.0, Weight::Regular, 200.0, DIM);
    let rows: Vec<String> = SETTINGS_ROWS.iter().map(|r| settings_row(&app.settings, *r)).collect();
    let pos = anim.drum("settings", rows.len(), focus);
    drum(canvas, &rows, pos, TRACK, FG);
    dots(focus, rows.len(), canvas);
}

fn settings_row(s: &Settings, row: SettingsRow) -> String {
    match row {
        SettingsRow::Hints => format!("Helper text: {}", if s.hints { "On" } else { "Off" }),
        SettingsRow::Idle => format!(
            "Home screen: {}",
            match s.idle_s {
                None => "Never".to_owned(),
                Some(n) if n.is_multiple_of(60) => format!("{} min", n / 60),
                Some(n) => format!("{n} s"),
            }
        ),
        SettingsRow::Clock => format!("Clock: {}", if s.clock24 { "24-hour" } else { "12-hour" }),
        SettingsRow::Network => "Network".into(),
        SettingsRow::Back => "Back to Controls".into(),
    }
}

/// The idle home screen: the time, large, and the date.
fn home(app: &App, canvas: &mut Canvas) {
    let c = &app.clock;
    let (time, suffix) = if app.settings.clock24 {
        (format!("{:02}:{:02}", c.hour, c.minute), "")
    } else {
        let h = if c.hour.is_multiple_of(12) { 12 } else { c.hour % 12 };
        (format!("{h}:{:02}", c.minute), if c.hour < 12 { "AM" } else { "PM" })
    };
    let (big, small) = (124.0, 30.0);
    let w_time = canvas.fonts.width(&time, big, Weight::Light);
    let w_suffix = if suffix.is_empty() { 0.0 } else { 10.0 + canvas.fonts.width(suffix, small, Weight::Regular) };
    let left = C - (w_time + w_suffix) / 2.0;
    let baseline = 226.0 + big * 0.727 / 2.0;
    canvas.text(left, baseline, &time, big, Weight::Light, FG);
    if !suffix.is_empty() {
        canvas.text(left + w_time + 10.0, baseline, suffix, small, Weight::Regular, DIM);
    }
    canvas.text_centred(C, 318.0, &c.date, 24.0, Weight::Regular, 340.0, DIM);
}

/// What the caller knows of the network, a label above each value.
fn network(app: &App, canvas: &mut Canvas) {
    canvas.text_centred(C, 70.0, "Network", 30.0, Weight::Bold, 300.0, FG);
    let rows = &app.network;
    if rows.is_empty() {
        canvas.text_centred(C, 240.0, "Nothing known yet", 24.0, Weight::Regular, 320.0, DIM);
    }
    let n = rows.len().min(5) as f32;
    let top = 240.0 - 31.0 * (n - 1.0);
    for (i, (label, value)) in rows.iter().take(5).enumerate() {
        let y = top + 62.0 * i as f32;
        canvas.text_centred(C, y - 12.0, label, 16.0, Weight::Regular, 300.0, DIM);
        canvas.text_centred(C, y + 12.0, value, 24.0, Weight::Bold, 340.0, FG);
    }
    hint(canvas, 430.0, "Click to go back");
}

/// rows as a drum turned to `pos`: the row there in the middle on a pill
/// of `pill`, in `rgb`; the others above and below, smaller and fainter the
/// further they have turned away.
fn drum(canvas: &mut Canvas, rows: &[String], pos: f32, pill: u32, rgb: u32) {
    let n = rows.len() as i64;
    if n == 0 {
        return;
    }
    canvas.line(90.0, 240.0, 390.0, 240.0, 78.0, pill);
    // Each row once: a window of at most n rows around the middle.
    let base = pos.round() as i64;
    let (lo, hi) = if n >= 7 {
        (base - 3, base + 3)
    } else {
        let lo = base - ((n - 1) / 2).min(2);
        (lo, lo + n.min(5) - 1)
    };
    for k in lo..=hi {
        let o = k as f32 - pos;
        if o.abs() > 2.7 {
            continue;
        }
        let theta = (o * DRUM_STEP_DEG).to_radians();
        let (sin, cos) = theta.sin_cos();
        let y = C + DRUM_R * sin;
        // Smaller and fainter as it turns away; the middle row in rgb.
        let size = (40.0 * (0.3 + 0.7 * cos.powi(3)) * 2.0).round() / 2.0;
        let faded = mix(BG, DIM, cos * cos);
        let colour = mix(faded, rgb, (1.0 - o.abs()).clamp(0.0, 1.0));
        let half_chord = (C * C - (y - C) * (y - C)).max(0.0).sqrt();
        let width = (2.0 * half_chord - 70.0).clamp(60.0, 320.0);
        let text = &rows[k.rem_euclid(n) as usize];
        // A long row shrinks to fit (down to 70%) before it is cut short.
        let natural = canvas.fonts.width(text, size, Weight::Bold);
        let size = if natural > width { ((size * (width / natural).max(0.7)) * 2.0).floor() / 2.0 } else { size };
        canvas.text_centred(C, y, text, size, Weight::Bold, width, colour);
    }
}

/// Where the focus is among n: dots, or "i / n" when they would not fit.
fn dots(i: usize, n: usize, canvas: &mut Canvas) {
    if n <= 1 {
        return;
    }
    if n > 14 {
        canvas.text_centred(C, 446.0, &format!("{} / {}", i + 1, n), 18.0, Weight::Regular, 200.0, DIM);
        return;
    }
    let gap = 15.0;
    let left = C - gap * (n as f32 - 1.0) / 2.0;
    for k in 0..n {
        let (r, rgb) = if k == i { (4.5, FG) } else { (3.0, DIM2) };
        canvas.disc(left + gap * k as f32, 446.0, r, rgb);
    }
}

/// The track and fill of a dial or the volume around the face's edge.
fn edge(canvas: &mut Canvas, fraction: f32, rgb: u32, knob: bool) {
    canvas.arc(C, C, EDGE_R, EDGE_W, EDGE_START, EDGE_SWEEP, TRACK);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        canvas.arc(C, C, EDGE_R, EDGE_W, EDGE_START, EDGE_SWEEP * f, rgb);
    }
    if knob {
        let a = (EDGE_START + EDGE_SWEEP * f).to_radians();
        canvas.disc(C + EDGE_R * a.cos(), C + EDGE_R * a.sin(), EDGE_W / 2.0 + 4.0, FG);
    }
}

fn range_labels(canvas: &mut Canvas, c: &Control, min: f64, max: f64) {
    let lo = c.string_min.clone().unwrap_or_else(|| min.to_string());
    let hi = c.string_max.clone().unwrap_or_else(|| max.to_string());
    canvas.text_centred(C - 92.0, 420.0, &lo, 18.0, Weight::Regular, 110.0, DIM);
    canvas.text_centred(C + 92.0, 420.0, &hi, 18.0, Weight::Regular, 110.0, DIM);
}

fn fraction(c: &Control, min: f64, max: f64) -> f32 {
    ((c.value.as_f64().unwrap_or(min) - min) / (max - min)) as f32
}

/// A gain component: its level around the edge, its mute by a click.
fn volume(screen: &Screen, canvas: &mut Canvas) {
    let Layout::Volume { gain, mute } = screen.layout else { return };
    let c = &screen.items[gain].control;
    let muted = screen.muted();
    let (min, max) = c.range().unwrap_or((0.0, 1.0));
    edge(canvas, fraction(c, min, max), if muted { RED } else { ACCENT }, !muted);
    canvas.text_centred(C, 150.0, &screen.title, 34.0, Weight::Bold, 300.0, FG);
    let level = crate::app::percent(c).map_or_else(|| shown(c), |p| format!("{p}%"));
    canvas.text_centred(C, 246.0, &level, 96.0, Weight::Light, 320.0, if muted { DIM } else { FG });
    if muted {
        canvas.text_centred(C, 326.0, "Muted", 30.0, Weight::Bold, 200.0, RED);
        hint(canvas, 368.0, "Turn to unmute");
    } else if mute.is_some_and(|m| screen.items[m].shown()) {
        hint(canvas, 340.0, "Click to mute");
    }
}

fn control(app: &App, screen: &Screen, item: &Item, mode: Mode, canvas: &mut Canvas, anim: &mut Anim) {
    let c = &item.control;
    let editing = matches!(mode, Mode::Edit { .. });
    match &item.widget {
        Widget::Dial { min, max } | Widget::Meter { min, max } => {
            let meter = matches!(item.widget, Widget::Meter { .. });
            let rgb = if meter { FG } else if editing { EDIT } else { ACCENT };
            edge(canvas, fraction(c, *min, *max), rgb, !meter);
            canvas.text_centred(C, 108.0, &screen.title, 20.0, Weight::Regular, 220.0, DIM);
            canvas.text_centred(C, 150.0, &item.label, 32.0, Weight::Bold, 300.0, FG);
            canvas.text_centred(C, 250.0, &shown(c), 80.0, Weight::Light, 320.0, if editing { EDIT } else { FG });
            if !meter {
                hint(canvas, 336.0, if editing { "Turn to adjust · click when done" } else { "Click to adjust" });
            }
            range_labels(canvas, c, *min, *max);
        }
        Widget::NumberEntry => {
            header(canvas, &screen.title, &item.label);
            canvas.text_centred(C, 262.0, &shown(c), 80.0, Weight::Light, 360.0, if editing { EDIT } else { FG });
            hint(canvas, 400.0, if editing { "Turn to change · click when done" } else { "Click to change" });
        }
        Widget::Toggle => {
            header(canvas, &screen.title, &item.label);
            let on = c.is_on();
            let (x0, x1) = (C - 38.0, C + 38.0);
            canvas.line(x0, 262.0, x1, 262.0, 76.0, if on { ACCENT } else { TRACK });
            canvas.disc(if on { x1 } else { x0 }, 262.0, 30.0, if on { FG } else { DIM });
            canvas.text_centred(C, 352.0, &shown(c), 28.0, Weight::Regular, 300.0, if on { FG } else { DIM });
            hint(canvas, 400.0, "Click to switch");
        }
        Widget::Indicator => {
            header(canvas, &screen.title, &item.label);
            if c.is_on() {
                canvas.ring(C, 262.0, 62.0, 10.0, mix(BG, ACCENT, 0.35));
                canvas.disc(C, 262.0, 48.0, ACCENT);
            } else {
                canvas.disc(C, 262.0, 48.0, 0x161616);
                canvas.ring(C, 262.0, 48.0, 4.0, TRACK);
            }
            canvas.text_centred(C, 360.0, &shown(c), 28.0, Weight::Regular, 300.0, if c.is_on() { FG } else { DIM });
        }
        Widget::Momentary | Widget::Button => {
            header(canvas, &screen.title, &item.label);
            let momentary = item.widget == Widget::Momentary;
            let lit = if momentary {
                c.is_on()
            } else {
                app.flash.as_ref().is_some_and(|(comp, name, _)| *comp == screen.component.name && *name == c.name)
            };
            if lit {
                canvas.disc(C, 266.0, 80.0, ACCENT);
            } else {
                canvas.ring(C, 266.0, 77.0, 6.0, ACCENT);
            }
            let word = if momentary { "Hold" } else { "Press" };
            canvas.text_centred(C, 266.0, word, 30.0, Weight::Bold, 140.0, FG);
            hint(canvas, 392.0, if momentary { "On while held" } else { "Click to fire" });
        }
        Widget::Picker { choices } => match mode {
            Mode::Edit { choice } if !choices.is_empty() => {
                let key = format!("pick:{}:{}", screen.component.name, c.name);
                wheel(canvas, anim, &key, &item.label, choices, choice, EDIT);
                hint_sized(canvas, 430.0, "Click picks · hold cancels", 17.0);
            }
            _ => {
                header(canvas, &screen.title, &item.label);
                canvas.text_centred(C, 256.0, &shown(c), 44.0, Weight::Bold, 360.0, FG);
                let at = choices.iter().position(|ch| Some(ch.as_str()) == c.string.as_deref());
                if let Some(at) = at {
                    let pos = format!("{} of {}", at + 1, choices.len());
                    canvas.text_centred(C, 306.0, &pos, 20.0, Weight::Regular, 200.0, DIM);
                }
                hint(canvas, 392.0, "Click to choose");
            }
        },
        Widget::TextEntry | Widget::Label | Widget::Hidden => {
            header(canvas, &screen.title, &item.label);
            let text = shown(c);
            if text.is_empty() {
                canvas.text_centred(C, 262.0, "Empty", 28.0, Weight::Regular, 300.0, DIM2);
            } else {
                let lines = canvas.fonts.wrap(&text, 34.0, Weight::Regular, 340.0, 3);
                let top = 262.0 - 22.0 * (lines.len() as f32 - 1.0);
                for (i, line) in lines.iter().enumerate() {
                    canvas.text_centred(C, top + 44.0 * i as f32, line, 34.0, Weight::Regular, 360.0, FG);
                }
            }
        }
    }
}

/// A combo box's choices as a drum, rolled to `choice`, highlighted in rgb.
#[allow(clippy::too_many_arguments)]
fn wheel(canvas: &mut Canvas, anim: &mut Anim, key: &str, label: &str, choices: &[String], choice: usize, rgb: u32) {
    canvas.text_centred(C, 70.0, label, 22.0, Weight::Regular, 260.0, DIM);
    let pos = anim.drum(key, choices.len(), choice);
    drum(canvas, choices, pos, mix(BG, rgb, 0.22), rgb);
}

/// A component whose one control is a combo box: its choices always on a
/// wheel, the current one at rest, the one a click would pick while turning.
fn selector(screen: &Screen, mode: Mode, canvas: &mut Canvas, anim: &mut Anim) {
    let Layout::Selector { item } = screen.layout else { return };
    let c = &screen.items[item].control;
    let Widget::Picker { choices } = &screen.items[item].widget else { return };
    if choices.is_empty() {
        return;
    }
    let key = format!("pick:{}:{}", screen.component.name, c.name);
    match mode {
        Mode::Edit { choice } => {
            wheel(canvas, anim, &key, &screen.title, choices, choice.min(choices.len() - 1), EDIT);
            hint_sized(canvas, 430.0, "Click to select", 18.0);
        }
        _ => {
            let current = choices.iter().position(|ch| Some(ch.as_str()) == c.string.as_deref());
            match current {
                Some(at) => wheel(canvas, anim, &key, &screen.title, choices, at, ACCENT),
                // A value not among the choices: shown on its own.
                None => {
                    canvas.text_centred(C, 70.0, &screen.title, 22.0, Weight::Regular, 260.0, DIM);
                    canvas.text_centred(C, 240.0, &shown(c), 40.0, Weight::Bold, 360.0, FG);
                }
            }
            hint_sized(canvas, 430.0, "Turn to choose", 18.0);
        }
    }
}

/// The stop after a screen's last control: back to the list.
fn controls_stop(screen: &Screen, canvas: &mut Canvas) {
    header(canvas, &screen.title, "Controls");
    canvas.ring(C, 266.0, 77.0, 6.0, ACCENT);
    // A chevron pointing left.
    canvas.line(C + 10.0, 236.0, C - 18.0, 266.0, 10.0, FG);
    canvas.line(C - 18.0, 266.0, C + 10.0, 296.0, 10.0, FG);
    hint(canvas, 392.0, "Click for all controls");
}
