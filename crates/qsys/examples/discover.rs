//! The Rust client's discovery against a real Core or Designer's emulation,
//! as tools/qsys/qrc_discover.py does it:
//!
//!     cargo run -p qsys --example discover -- HOST [--port N] [--prefix P]
//!         [--user U --password P] [--set COMP.CTRL=VALUE]... [--watch SECS]

use std::process::ExitCode;
use std::time::{Duration, Instant};

use qsys::{pick, Client, Qrc, Transport, DEFAULT_PORT, PREFIX};
use serde_json::Value;

struct Args {
    host: String,
    port: u16,
    prefix: String,
    user: Option<String>,
    password: String,
    sets: Vec<String>,
    watch: u64,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        host: String::new(),
        port: DEFAULT_PORT,
        prefix: PREFIX.into(),
        user: None,
        password: String::new(),
        sets: Vec::new(),
        watch: 0,
    };
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--port" => a.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--prefix" => a.prefix = value()?,
            "--user" => a.user = Some(value()?),
            "--password" => a.password = value()?,
            "--set" => a.sets.push(value()?),
            "--watch" => a.watch = value()?.parse().map_err(|e| format!("--watch: {e}"))?,
            "--all" => a.prefix.clear(),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => a.host = s.into(),
        }
    }
    if a.host.is_empty() {
        return Err("usage: discover HOST [--port N] [--prefix P | --all] [--user U --password P] \
                    [--set COMP.CTRL=VALUE]... [--watch SECS]"
            .into());
    }
    Ok(a)
}

fn cell(s: &str, width: usize) -> String {
    if s.chars().count() > width {
        s.chars().take(width - 1).chain(['~']).collect()
    } else {
        format!("{s:width$}")
    }
}

fn opt(v: Option<f64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn run(a: Args) -> qsys::Result<()> {
    let mut q = Qrc::new(Client::connect(&a.host, a.port, Duration::from_secs(5))?);
    if let Some(user) = &a.user {
        q.client.logon(user, &a.password)?;
        println!("logged on as {user}");
    }
    for spec in &a.sets {
        let parsed = spec.split_once('=').and_then(|(t, v)| t.rsplit_once('.').map(|(c, n)| (c, n, v)));
        let Some((comp, ctrl, text)) = parsed else {
            eprintln!("--set wants COMP.CTRL=VALUE, got {spec:?}");
            continue;
        };
        let value = serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.into()));
        q.set(comp, ctrl, value.clone())?;
        println!("set {comp}.{ctrl} = {value}");
    }
    let found = q.list(&a.prefix)?;
    println!("{} components match {:?}", found.len(), a.prefix);
    if found.is_empty() {
        println!("Nothing to show. Check: Code Name set, Script Access = External/All, prefix.");
    }
    for d in &found {
        println!("\n== {}  ({} controls)", d.component.name, d.controls.len());
        println!("{}{}{}{}{}{}{}notes", cell("control", 26), cell("type", 9), cell("value", 12),
                 cell("min", 8), cell("max", 8), cell("dir", 11), cell("widget", 14));
        for c in &d.controls {
            let (w, notes) = pick(c);
            let notes: Vec<&str> = notes.iter().map(|n| n.text()).collect();
            let dir = format!("{:?}", c.direction);
            println!("{}{}{}{}{}{}{}{}", cell(&c.name, 26), cell(c.type_name.as_deref().unwrap_or(""), 9),
                     cell(&c.display(), 12), cell(&opt(c.min), 8), cell(&opt(c.max), 8), cell(&dir, 11),
                     cell(w.name(), 14), notes.join("; "));
        }
    }
    if a.watch > 0 {
        for d in &found {
            let names: Vec<&str> = d.controls.iter().map(|c| c.name.as_str()).collect();
            q.subscribe(&d.component.name, &names)?;
        }
        println!("\nwatching for {}s...", a.watch);
        let end = Instant::now() + Duration::from_secs(a.watch);
        let mut last_noop = Instant::now();
        while Instant::now() < end {
            for ch in q.poll()? {
                let shown = ch.string.clone().unwrap_or_else(|| ch.value.to_string());
                println!("  {}.{} = {}", ch.component.as_deref().unwrap_or("?"), ch.name, shown);
            }
            if last_noop.elapsed() > Duration::from_secs(20) {
                q.client.noop()?;
                last_noop = Instant::now();
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
