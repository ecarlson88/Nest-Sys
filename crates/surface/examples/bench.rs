//! How long frames take to draw: each kind of screen, and a drum rolling.
//!
//!     cargo run --release -p surface --example bench

use std::time::Instant;

use qsys::{Component, Control, Discovered, PREFIX};
use serde_json::json;
use surface::app::{App, Mode, SETTINGS};
use surface::canvas::{Canvas, PixelFormat, FACE};
use surface::render::Renderer;

fn control(v: serde_json::Value) -> Control {
    Control::from_json(&v).unwrap()
}

fn design() -> App {
    let gain = Discovered {
        component: Component { name: "NEST_Zone".into(), kind: Some("gain".into()) },
        controls: vec![
            control(json!({"Name": "gain", "Type": "Float", "Value": -20.0, "ValueMin": -100.0, "ValueMax": 20.0,
                           "String": "-20.0dB", "Direction": "Read/Write"})),
            control(json!({"Name": "mute", "Type": "Boolean", "Value": false, "Direction": "Read/Write"})),
        ],
    };
    let mut combo = control(json!({"Name": "source", "Type": "Text", "String": "Mic 1", "Direction": "Read/Write"}));
    combo.choices = Some(["Mic 1", "Mic 2", "Laptop", "Streaming", "Radio", "TV"].map(String::from).to_vec());
    let pick = Discovered {
        component: Component { name: "NEST_Source".into(), kind: Some("device_controller_script".into()) },
        controls: vec![combo],
    };
    let panel = Discovered {
        component: Component { name: "NEST_Panel".into(), kind: Some("custom_controls".into()) },
        controls: vec![
            control(json!({"Name": "toggle.1", "Type": "Boolean", "Value": true, "Direction": "Read/Write"})),
            control(json!({"Name": "text.1", "Type": "Text", "String": "Hello there", "Direction": "Read/Write"})),
        ],
    };
    App::new(vec![gain, pick, panel], PREFIX)
}

fn time(label: &str, frames: u32, mut f: impl FnMut(u32)) {
    let t = Instant::now();
    for i in 0..frames {
        f(i);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!("{label:<28} {ms:7.2} ms/frame");
}

fn main() {
    let mut app = design();
    let mut canvas = Canvas::new(FACE);
    let mut r = Renderer::new();
    let mut now = 0u64;

    app.screen = 0;
    time("volume, still", 50, |_| {
        now += 16;
        r.frame(&app, &mut canvas, now);
    });
    app.screen = 2;
    time("toggle, still", 50, |_| {
        now += 16;
        r.frame(&app, &mut canvas, now);
    });
    app.screen = 1;
    time("selector, rolling", 50, |i| {
        now += 16;
        if i % 12 == 0 {
            app.mode = Mode::Edit { choice: (i / 12) as usize % 6 };
        }
        r.frame(&app, &mut canvas, now);
    });
    app.mode = Mode::Menu { focus: SETTINGS };
    time("controls list, still", 50, |_| {
        now += 16;
        r.frame(&app, &mut canvas, now);
    });
    time("to panel (XRGB8888)", 50, |_| {
        std::hint::black_box(canvas.to_panel(&PixelFormat::XRGB8888, FACE * 4));
    });
    time("to panel (RGB565)", 50, |_| {
        std::hint::black_box(canvas.to_panel(&PixelFormat::RGB565, FACE * 2));
    });
    time("png (simulator only)", 20, |_| {
        std::hint::black_box(canvas.to_png());
    });
}
