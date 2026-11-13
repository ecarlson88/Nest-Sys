//! Draws the app on the round face.

use qsys::{Control, Widget};
use serde_json::Value;

use crate::app::{App, Item, Mode};
use crate::canvas::{Canvas, FACE};

const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const DIM: u32 = 0x8c8c8c;
const FAINT: u32 = 0x2e2e2e;
const ACCENT: u32 = 0x2fa8ff;
const EDIT: u32 = 0xff8a1f;

const C: i32 = FACE as i32 / 2;
/// Text must stay inside the circle; this much width is safe mid-face.
const WIDE: usize = 380;

/// The value as shown: the Core's String, else the value itself.
fn shown(c: &Control) -> String {
    if let Some(s) = c.string.as_deref().filter(|s| !s.is_empty()) {
        return s.to_owned();
    }
    match &c.value {
        Value::Bool(b) => if *b { "on" } else { "off" }.into(),
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
        (None, _) => {
            canvas.text_centred(C, 200, "No NEST_", 3, WIDE, FG);
            canvas.text_centred(C, 240, "components", 3, WIDE, FG);
            canvas.text_centred(C, 300, "Code Name + Script Access", 1, WIDE, DIM);
        }
        (Some(_), Mode::Menu { focus }) => menu(app, focus, canvas),
        (Some(screen), mode) => {
            canvas.text_centred(C, 58, &screen.title, 2, 300, DIM);
            if let Some(item) = app.focused() {
                canvas.text_centred(C, 96, &item.label, 3, WIDE, FG);
                widget(item, mode, canvas);
            }
            dots(app.focus, screen.items.len(), canvas);
        }
    }
    canvas.mask_round();
}

fn menu(app: &App, focus: usize, canvas: &mut Canvas) {
    canvas.text_centred(C, 58, "Components", 2, 300, DIM);
    let n = app.screens.len();
    if n > 1 {
        let prev = (focus + n - 1) % n;
        canvas.text_centred(C, 168, &app.screens[prev].title, 2, WIDE, DIM);
    }
    canvas.fill_rect(70, 214, 340, 52, FAINT);
    canvas.text_centred(C, 228, &app.screens[focus].title, 3, 330, FG);
    if n > 2 {
        let next = (focus + 1) % n;
        canvas.text_centred(C, 296, &app.screens[next].title, 2, WIDE, DIM);
    }
    canvas.text_centred(C, 380, "click opens, hold goes back", 1, WIDE, DIM);
    dots(focus, n, canvas);
}

/// Where the focus is among n: dots, or "i / n" when they would not fit.
fn dots(i: usize, n: usize, canvas: &mut Canvas) {
    if n <= 1 {
        return;
    }
    if n > 14 {
        canvas.text_centred(C, 420, &format!("{} / {}", i + 1, n), 2, WIDE, DIM);
        return;
    }
    let gap = 16;
    let left = C - (gap * (n as i32 - 1)) / 2;
    for k in 0..n {
        let (r, rgb) = if k == i { (5.0, ACCENT) } else { (3.5, DIM) };
        canvas.disc(left + gap * k as i32, 428, r, 0.0, rgb);
    }
}

fn widget(item: &Item, mode: Mode, canvas: &mut Canvas) {
    let c = &item.control;
    let editing = matches!(mode, Mode::Edit { .. });
    let accent = if editing { EDIT } else { ACCENT };
    match &item.widget {
        Widget::Dial { min, max } | Widget::Meter { min, max } => {
            let meter = matches!(item.widget, Widget::Meter { .. });
            let v = c.value.as_f64().unwrap_or(*min);
            let f = ((v - min) / (max - min)).clamp(0.0, 1.0) as f32;
            let (outer, inner) = if meter { (128.0, 120.0) } else { (130.0, 114.0) };
            canvas.arc(C, 262, outer, inner, 135.0, 270.0, FAINT);
            if f > 0.0 {
                canvas.arc(C, 262, outer, inner, 135.0, 270.0 * f, if meter { FG } else { accent });
            }
            canvas.text_centred(C, 236, &shown(c), 4, 220, FG);
            let lo = c.string_min.clone().unwrap_or_else(|| min.to_string());
            let hi = c.string_max.clone().unwrap_or_else(|| max.to_string());
            canvas.text_centred(C - 92, 372, &lo, 1, 90, DIM);
            canvas.text_centred(C + 92, 372, &hi, 1, 90, DIM);
            if !meter {
                hint(canvas, if editing { "turn to adjust, click when done" } else { "click to adjust" });
            }
        }
        Widget::NumberEntry => {
            canvas.text_centred(C, 236, &shown(c), 4, WIDE, if editing { EDIT } else { FG });
            hint(canvas, if editing { "turn to change, click when done" } else { "click to change" });
        }
        Widget::Toggle | Widget::Indicator | Widget::Momentary => {
            let on = c.is_on();
            if on {
                canvas.disc(C, 250, 62.0, 0.0, accent);
            } else {
                canvas.disc(C, 250, 62.0, 52.0, FAINT);
            }
            canvas.text_centred(C, 336, &shown(c), 2, WIDE, if on { FG } else { DIM });
            match item.widget {
                Widget::Toggle => hint(canvas, "click to switch"),
                Widget::Momentary => hint(canvas, "on while the click is down"),
                _ => {}
            }
        }
        Widget::Button => {
            canvas.disc(C, 250, 62.0, 54.0, ACCENT);
            canvas.text_centred(C, 242, "PRESS", 2, 120, FG);
            hint(canvas, "click to fire");
        }
        Widget::Picker { choices } => match mode {
            Mode::Edit { choice } if !choices.is_empty() => {
                let n = choices.len();
                if n > 1 {
                    canvas.text_centred(C, 176, &choices[(choice + n - 1) % n], 2, WIDE, DIM);
                }
                canvas.fill_rect(70, 222, 340, 52, FAINT);
                canvas.text_centred(C, 236, &choices[choice], 3, 330, EDIT);
                if n > 2 {
                    canvas.text_centred(C, 304, &choices[(choice + 1) % n], 2, WIDE, DIM);
                }
                hint(canvas, "click picks, hold cancels");
            }
            _ => {
                canvas.text_centred(C, 236, &shown(c), 3, WIDE, FG);
                hint(canvas, &format!("click to choose ({})", choices.len()));
            }
        },
        Widget::TextEntry | Widget::Label | Widget::Hidden => {
            let text = shown(c);
            if text.is_empty() {
                canvas.text_centred(C, 240, "(empty)", 2, WIDE, DIM);
            } else {
                canvas.text_centred(C, 236, &text, 3, WIDE, FG);
            }
        }
    }
}

fn hint(canvas: &mut Canvas, text: &str) {
    canvas.text_centred(C, 396, text, 1, 300, DIM);
}
