//! The control surface on the unit itself: the framebuffer and the ring in
//! place of the simulator's browser page.
//!
//!     nest-surface [CORE_HOST] [--config /media/data/nest-surface/config.json]
//!         [--port 1710] [--user U --password P] [--prefix NEST_]
//!         [--settings /media/data/nest-surface/settings.json]
//!         [--fb /dev/fb0] [--input /dev/input]
//!
//! The Core's address and the rest come from the config file, a JSON
//! object {"core": "192.168.1.20", "port": 1710, "user": "...",
//! "password": "...", "prefix": "NEST_"}; options given override it. It
//! runs until SIGTERM or SIGINT, then puts back what the display showed.

use std::process::ExitCode;

#[cfg(target_os = "linux")]
mod unit {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    use serde_json::Value;
    use surface::device::{stop_flag, Console, Framebuffer, Inputs};
    use surface::runner::{Config, Runner};

    const DATA: &str = "/media/data/nest-surface";

    pub struct Args {
        config: Config,
        fb: PathBuf,
        input: PathBuf,
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
        let mut a = Args { config, fb: "/dev/fb0".into(), input: "/dev/input".into() };
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
                s if s.starts_with('-') => return Err(format!("unknown option {s}")),
                s => a.config.host = s.into(),
            }
        }
        if a.config.host.is_empty() {
            return Err(format!("no Core: give its address, or \"core\" in {file}"));
        }
        Ok(a)
    }

    pub fn run(a: Args) -> std::io::Result<()> {
        let stop = stop_flag();
        let at = |path: &PathBuf| move |e: std::io::Error| std::io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        let mut fb = Framebuffer::open(&a.fb).map_err(at(&a.fb))?;
        let _console = Console::graphics();
        let mut inputs = Inputs::open(&a.input).map_err(at(&a.input))?;
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
