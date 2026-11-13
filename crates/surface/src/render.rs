//! Draws the app on the round face.

use qsys::{Control, Widget};
use serde_json::Value;

use crate::app::{App, Item, Layout, Mode, Screen};
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

pub fn render(app: &App, canvas: &mut Canvas) {
    canvas.clear(BG);
    match (app.current(), app.mode) {
        (None, _) => status(canvas, "No controls", "Give components a NEST_ Code Name and Script Access External"),
        (Some(_), Mode::Menu { focus }) => menu(app, focus, canvas),
        (Some(screen), _) if matches!(screen.layout, Layout::Volume { .. }) => volume(screen, canvas),
        (Some(screen), mode) if matches!(screen.layout, Layout::Selector { .. }) => selector(screen, mode, canvas),
        (Some(screen), mode) => {
            match app.focused() {
                Some(item) => control(app, screen, item, mode, canvas),
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

fn hint(canvas: &mut Canvas, y: f32, text: &str) {
    canvas.text_centred(C, y, text, 19.0, Weight::Regular, 300.0, DIM);
}

/// The list of components, as a wheel of names around the one shown.
fn menu(app: &App, focus: usize, canvas: &mut Canvas) {
    canvas.text_centred(C, 58.0, "Controls", 20.0, Weight::Regular, 200.0, DIM);
    let shown = app.shown_screens();
    let n = shown.len();
    let at = shown.iter().position(|&i| i == focus).unwrap_or(0);
    canvas.line(90.0, 240.0, 390.0, 240.0, 78.0, TRACK);
    canvas.text_centred(C, 240.0, &app.screens[focus].title, 40.0, Weight::Bold, 320.0, FG);
    for (offset, i) in neighbours(n, at) {
        let title = app.screens[shown[i]].title.clone();
        wheel_row(canvas, offset, &title);
    }
    dots(at, n, canvas);
}

/// The rows around the one at `at` of n in a wheel, as (offset, index):
/// up to two each way, each index once, the nearest first.
fn neighbours(n: usize, at: usize) -> Vec<(i32, usize)> {
    let mut used = vec![at];
    let mut rows = Vec::new();
    for offset in [1i32, -1, 2, -2] {
        if n == 0 {
            break;
        }
        let i = (at as i64 + offset as i64).rem_euclid(n as i64) as usize;
        if !used.contains(&i) {
            used.push(i);
            rows.push((offset, i));
        }
    }
    rows
}

/// A row of a wheel: further from the middle is smaller and fainter.
fn wheel_row(canvas: &mut Canvas, offset: i32, text: &str) {
    let (dy, size, rgb) = if offset.abs() == 1 { (74.0, 28.0, DIM) } else { (134.0, 22.0, DIM2) };
    canvas.text_centred(C, 240.0 + dy * offset.signum() as f32, text, size, Weight::Regular, 300.0, rgb);
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

fn control(app: &App, screen: &Screen, item: &Item, mode: Mode, canvas: &mut Canvas) {
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
            Mode::Edit { choice } if !choices.is_empty() => picker_wheel(canvas, &item.label, choices, choice),
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

/// A combo box being chosen from: the choices as a wheel around the one
/// that a click would pick.
fn picker_wheel(canvas: &mut Canvas, label: &str, choices: &[String], choice: usize) {
    wheel(canvas, label, choices, choice, EDIT);
    canvas.text_centred(C, 430.0, "Click picks · hold cancels", 17.0, Weight::Regular, 260.0, DIM);
}

/// choices as a wheel around `choice`, highlighted in rgb.
fn wheel(canvas: &mut Canvas, label: &str, choices: &[String], choice: usize, rgb: u32) {
    canvas.text_centred(C, 70.0, label, 22.0, Weight::Regular, 260.0, DIM);
    canvas.line(90.0, 240.0, 390.0, 240.0, 78.0, mix(BG, rgb, 0.22));
    canvas.text_centred(C, 240.0, &choices[choice], 40.0, Weight::Bold, 320.0, rgb);
    for (offset, i) in neighbours(choices.len(), choice) {
        wheel_row(canvas, offset, &choices[i]);
    }
}

/// A component whose one control is a combo box: its choices always on a
/// wheel, the current one at rest, the one a click would pick while turning.
fn selector(screen: &Screen, mode: Mode, canvas: &mut Canvas) {
    let Layout::Selector { item } = screen.layout else { return };
    let c = &screen.items[item].control;
    let Widget::Picker { choices } = &screen.items[item].widget else { return };
    if choices.is_empty() {
        return;
    }
    match mode {
        Mode::Edit { choice } => {
            wheel(canvas, &screen.title, choices, choice.min(choices.len() - 1), EDIT);
            canvas.text_centred(C, 430.0, "Click to select", 18.0, Weight::Regular, 260.0, DIM);
        }
        _ => {
            let current = choices.iter().position(|ch| Some(ch.as_str()) == c.string.as_deref());
            match current {
                Some(at) => wheel(canvas, &screen.title, choices, at, ACCENT),
                // A value not among the choices: shown on its own.
                None => {
                    canvas.text_centred(C, 70.0, &screen.title, 22.0, Weight::Regular, 260.0, DIM);
                    canvas.text_centred(C, 240.0, &shown(c), 40.0, Weight::Bold, 360.0, FG);
                }
            }
            canvas.text_centred(C, 430.0, "Turn to choose", 18.0, Weight::Regular, 260.0, DIM);
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
