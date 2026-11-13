//! The control surface on the unit itself: the framebuffer and the ring in
//! place of the simulator's browser page.
//!
//!     nest-surface [CORE_HOST] [--config /media/data/nest-surface/config.json]
//!         [--port 1710] [--user U --password P] [--prefix NEST_]
//!         [--settings /media/data/nest-surface/settings.json]
//!         [--fb /dev/fb0] [--input /dev/input]
//!     nest-surface --diag [--fb /dev/fb0] [--input /dev/input]
//!
//! The Core's address and the rest come from the config file, a JSON
//! object {"core": "192.168.1.20", "port": 1710, "user": "...",
//! "password": "...", "prefix": "NEST_"}; options given override it. It
//! runs until SIGTERM or SIGINT, then puts back what the display showed.
//!
//! --diag needs no Core: it prints what the framebuffer, backlight and
//! input devices report, shows a test pattern through the same panel path
//! as the screen (surface::diag) and times it, then logs and counts every
//! turn of the ring and press of the click until stopped.

use std::process::ExitCode;

#[cfg(target_os = "linux")]
mod unit {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    use std::time::Instant;

    use serde_json::Value;
    use surface::canvas::{Canvas, FACE};
    use surface::device::{describe, describe_backlights, stop_flag, Console, Framebuffer, Inputs};
    use surface::diag::Diag;
    use surface::input::{Button, Event};
    use surface::runner::{Config, Input, Runner};

    const DATA: &str = "/media/data/nest-surface";

    pub struct Args {
        config: Config,
        fb: PathBuf,
        input: PathBuf,
        diag: bool,
    }

    /// The config file's fields into config, where the command line has
    /// not set them.
    fn read_config(path: &str, config: &mut Config) -> Result<(), String> {
        let text = match std::fs::read(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{path}: {e}")),
        };
        let v: Value = serde_json::from_slice(&text).map_err(|e| format!("{path}: {e}"))?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        if let Some(core) = s("core") {
            config.host = core;
        }
        if let Some(port) = v.get("port").and_then(Value::as_u64) {
            config.port = u16::try_from(port).map_err(|_| format!("{path}: port {port}"))?;
        }
        if let Some(user) = s("user") {
            config.user = Some(user);
        }
        if let Some(password) = s("password") {
            config.password = password;
        }
        if let Some(prefix) = s("prefix") {
            config.prefix = prefix;
        }
        Ok(())
    }

    pub fn parse() -> Result<Args, String> {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        // The config file first, so that options given override it.
        let file = argv
            .windows(2)
            .find(|w| w[0] == "--config")
            .map_or_else(|| format!("{DATA}/config.json"), |w| w[1].clone());
        let mut config = Config::new("", &format!("{DATA}/settings.json"));
        read_config(&file, &mut config)?;
        let mut a = Args { config, fb: "/dev/fb0".into(), input: "/dev/input".into(), diag: false };
        let mut it = argv.into_iter();
        while let Some(arg) = it.next() {
            let mut value = || it.next().ok_or(format!("{arg} needs a value"));
            if a.config.option(&arg, &mut value)? {
                continue;
            }
            match arg.as_str() {
                "--config" => {
                    value()?;
                }
                "--fb" => a.fb = value()?.into(),
                "--input" => a.input = value()?.into(),
                "--diag" => a.diag = true,
                s if s.starts_with('-') => return Err(format!("unknown option {s}")),
                s => a.config.host = s.into(),
            }
        }
        if a.config.host.is_empty() && !a.diag {
            return Err(format!("no Core: give its address, or \"core\" in {file}"));
        }
        Ok(a)
    }

    /// e, saying which path it came from.
    fn at(path: &std::path::Path, e: std::io::Error) -> std::io::Error {
        std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
    }

    pub fn run(a: Args) -> std::io::Result<()> {
        if a.diag {
            return diag(a);
        }
        let stop = stop_flag();
        let mut fb = Framebuffer::open(&a.fb).map_err(|e| at(&a.fb, e))?;
        let _console = Console::graphics();
        let mut inputs = Inputs::open(&a.input).map_err(|e| at(&a.input, e))?;
        println!("nest-surface: {:?} panel, Core {}:{}", fb.panel(), a.config.host, a.config.port);
        let mut runner = Runner::new(a.config);
        while !stop.load(Ordering::SeqCst) {
            // Quick rounds while something moves, else wait on the ring.
            let wait = if runner.animating() { 8 } else { 30 };
            for input in inputs.wait(wait) {
                runner.input(input);
            }
            if runner.step() {
                fb.show(&runner.canvas);
            }
        }
        // fb and _console put the display back as they go.
        Ok(())
    }

    /// Prints a heading, and text indented under it.
    fn section(heading: &str, text: &str) {
        println!("{heading}:");
        for line in text.lines() {
            println!("  {line}");
        }
    }

    /// Times in milliseconds, summed up.
    fn spread(ms: &[f64]) -> String {
        let (min, max) = ms.iter().fold((f64::MAX, 0.0f64), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        let mean = ms.iter().sum::<f64>() / ms.len().max(1) as f64;
        format!("{mean:.1} ms on average ({min:.1} to {max:.1})")
    }

    fn diag(a: Args) -> std::io::Result<()> {
        let stop = stop_flag();
        println!("nest-surface --diag, version {}", env!("CARGO_PKG_VERSION"));
        match describe(&a.fb) {
            Ok(d) => section(&format!("{} as found", a.fb.display()), &d),
            Err(e) => println!("{}: {e}", a.fb.display()),
        }
        section("backlight", &describe_backlights());
        let inputs = Inputs::open(&a.input);
        match &inputs {
            Ok(i) => {
                let list: Vec<String> = i.names.iter().map(|(file, what)| format!("{file}: {what}")).collect();
                section(&a.input.display().to_string(), &list.join("\n"));
            }
            Err(e) => println!("{}: {e}", a.input.display()),
        }
        // Without a display it can show nothing, but the ring is still worth
        // reading, and the other way round.
        let mut fb = match Framebuffer::open(&a.fb) {
            Ok(fb) => Some(fb),
            Err(e) if inputs.is_ok() => {
                println!("{}: {e}; no test pattern, only the ring and click", a.fb.display());
                None
            }
            Err(e) => return Err(at(&a.fb, e)),
        };
        let _console = fb.is_some().then(Console::graphics);
        let mut canvas = Canvas::new(FACE);
        let mut d = Diag::default();
        if let Some(fb) = &mut fb {
            let heading = format!("{} in use: {:?} panel, {} page(s)", a.fb.display(), fb.panel(), fb.pages());
            section(&heading, &fb.describe().unwrap_or_else(|e| e.to_string()));
            // How long a frame takes to draw, and to lay out and show.
            let (mut draw, mut show) = (Vec::new(), Vec::new());
            for _ in 0..30 {
                let t = Instant::now();
                d.draw(&mut canvas);
                draw.push(t.elapsed().as_secs_f64() * 1000.0);
                let t = Instant::now();
                fb.show(&canvas);
                show.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("drawing the pattern: {}", spread(&draw));
            println!("laying it out and showing it: {}", spread(&show));
        }

        let Ok(mut inputs) = inputs else {
            println!("showing the pattern without the ring; Ctrl-C to stop");
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            return Ok(());
        };
        inputs.raw = Some(Vec::new());
        println!("turn the ring and press it; Ctrl-C to stop");
        let start = Instant::now();
        let mut button = Button::default();
        while !stop.load(Ordering::SeqCst) {
            let got = inputs.wait(30);
            let now = start.elapsed();
            let t = now.as_millis() as u64;
            let stamp = format!("[{:8.3}]", now.as_secs_f64());
            for (device, raw) in inputs.raw.as_mut().map(std::mem::take).unwrap_or_default() {
                println!("{stamp} {}: {raw}", inputs.names[device].0);
            }
            let mut events = Vec::new();
            for input in got {
                match input {
                    Input::Turn(n) => events.push(Event::Turn(n)),
                    Input::Down => events.extend(button.down(t)),
                    Input::Up => events.extend(button.up(t)),
                }
            }
            events.extend(button.tick(t));
            if events.is_empty() {
                continue;
            }
            for ev in events {
                println!("{stamp}   {}", d.handle(ev));
            }
            if let Some(fb) = &mut fb {
                d.draw(&mut canvas);
                fb.show(&canvas);
            }
        }
        println!(
            "stopped: {} detents ({} clockwise, {} anticlockwise), {} clicks, {} holds",
            d.detents, d.clockwise, d.anticlockwise, d.clicks, d.holds
        );
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    let args = match unit::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("nest-surface: {e}");
            return ExitCode::from(2);
        }
    };
    match unit::run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nest-surface: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("nest-surface runs on the unit; on a computer, use the simulator (sim)");
    ExitCode::from(2)
}
