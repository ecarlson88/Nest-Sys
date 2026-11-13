//! The screen against a real Core or Designer's emulation, shown in a
//! browser in place of the unit's display: the mouse wheel or the arrow
//! keys turn the ring, a mouse button or the space bar is the click.
//!
//!     cargo run -p surface --bin sim -- CORE_HOST [--port 1710]
//!         [--user U --password P] [--prefix NEST_] [--listen 127.0.0.1:8480]
//!
//! then open http://127.0.0.1:8480. When the Core goes away (Designer
//! leaving emulation, say), it shows "Connecting…" and tries again every
//! two seconds, discovering the design afresh when it is back.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use qsys::{Client, Qrc, Transport, DEFAULT_PORT, PREFIX};
use surface::app::{App, Set};
use surface::canvas::{Canvas, FACE};
use surface::input::{Button, Event};
use surface::render::{render, status};

const PAGE: &str = include_str!("sim.html");
const POLL_EVERY: Duration = Duration::from_millis(200);
const NOOP_EVERY: Duration = Duration::from_secs(20);

struct Args {
    host: String,
    port: u16,
    user: Option<String>,
    password: String,
    prefix: String,
    listen: String,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        host: String::new(),
        port: DEFAULT_PORT,
        user: None,
        password: String::new(),
        prefix: PREFIX.into(),
        listen: "127.0.0.1:8480".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--port" => a.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--user" => a.user = Some(value()?),
            "--password" => a.password = value()?,
            "--prefix" => a.prefix = value()?,
            "--listen" => a.listen = value()?,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => a.host = s.into(),
        }
    }
    if a.host.is_empty() {
        return Err("usage: sim CORE_HOST [--port N] [--user U --password P] [--prefix P] [--listen ADDR]".into());
    }
    Ok(a)
}

/// What the browser asked for.
enum Request {
    Page,
    Frame,
    Version,
    Input(String, i32),
    Other,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    let path = head.lines().next()?.split_whitespace().nth(1)?.to_owned();
    let (route, query) = path.split_once('?').unwrap_or((&path, ""));
    let param = |key: &str| {
        query.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')).map(str::to_owned))
    };
    Some(match route {
        "/" => Request::Page,
        "/frame.png" => Request::Frame,
        "/version" => Request::Version,
        "/input" => Request::Input(param("e").unwrap_or_default(), param("n").and_then(|n| n.parse().ok()).unwrap_or(0)),
        _ => Request::Other,
    })
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn now_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// A connection to the Core and the screen built from what it reported.
struct Session {
    q: Qrc<TcpStream>,
    app: App,
}

/// Connects, discovers the design and follows its controls.
fn connect(a: &Args) -> qsys::Result<Session> {
    let mut q = Qrc::new(Client::connect(&a.host, a.port, Duration::from_secs(3))?);
    if let Some(user) = &a.user {
        q.client.logon(user, &a.password)?;
    }
    let app = App::new(q.list(&a.prefix)?, &a.prefix);
    for (component, controls) in app.watched() {
        let names: Vec<&str> = controls.iter().map(String::as_str).collect();
        q.subscribe(&component, &names)?;
    }
    Ok(Session { q, app })
}

/// Whether an error means the connection is gone (rather than the Core
/// refusing one request).
fn lost(e: &qsys::Error) -> bool {
    !matches!(e, qsys::Error::Rpc { .. })
}

/// One round with the Core: the inputs, the values to send, the changes.
fn exchange(s: &mut Session, events: Vec<Event>, t: u64, poll: bool, noop: bool) -> qsys::Result<bool> {
    let mut dirty = s.app.tick(t);
    // A dial's turns are sent as one value per round of the loop.
    let mut sets: BTreeMap<(String, String), Set> = BTreeMap::new();
    for ev in events {
        for set in s.app.handle(ev, t) {
            sets.insert((set.component.clone(), set.control.clone()), set);
        }
        dirty = true;
    }
    for set in sets.into_values() {
        if let Err(e) = s.q.set(&set.component, &set.control, set.value) {
            if lost(&e) {
                return Err(e);
            }
            eprintln!("{}.{}: {e}", set.component, set.control);
        }
    }
    if poll {
        for change in s.q.poll()? {
            s.app.apply(&change);
            dirty = true;
        }
    }
    if noop {
        s.q.client.noop()?;
    }
    s.q.client.take_notifications();
    Ok(dirty)
}

const RETRY_EVERY: Duration = Duration::from_secs(2);

fn run(a: Args) -> std::io::Result<()> {
    let listener = TcpListener::bind(&a.listen)?;
    listener.set_nonblocking(true)?;
    println!("open http://{}", a.listen);
    let start = Instant::now();
    let mut button = Button::default();
    let mut canvas = Canvas::new(FACE);
    let mut frame = Vec::new();
    let mut version = 0u64;
    let mut dirty = true;
    let mut session: Option<Session> = None;
    // The screen to come back to after a reconnect, by component.
    let mut last_screen: Option<String> = None;
    let mut next_try = Instant::now();
    let mut trouble = String::from("Starting");
    let mut last_poll = Instant::now();
    let mut last_noop = Instant::now();

    loop {
        let mut events = Vec::new();
        while let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(false);
            match read_request(&mut stream) {
                Some(Request::Page) => respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes()),
                Some(Request::Frame) => respond(&mut stream, "200 OK", "image/png", &frame),
                Some(Request::Version) => respond(&mut stream, "200 OK", "text/plain", version.to_string().as_bytes()),
                Some(Request::Input(e, n)) => {
                    let t = now_ms(start);
                    match e.as_str() {
                        "turn" => events.push(Event::Turn(n)),
                        "down" => events.extend(button.down(t)),
                        "up" => events.extend(button.up(t)),
                        _ => {}
                    }
                    respond(&mut stream, "204 No Content", "text/plain", b"");
                }
                _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
            }
        }
        let t = now_ms(start);
        events.extend(button.tick(t));

        if session.is_none() && Instant::now() >= next_try {
            match connect(&a) {
                Ok(mut s) => {
                    println!("connected to {}:{}: {} screens", a.host, a.port, s.app.screens.len());
                    if let Some(name) = &last_screen {
                        if let Some(i) = s.app.screens.iter().position(|sc| &sc.component.name == name) {
                            s.app.screen = i;
                            s.app.focus = s.app.screens[i].stops()[0];
                        }
                    }
                    session = Some(s);
                    last_poll = Instant::now() - POLL_EVERY;
                    last_noop = Instant::now();
                }
                Err(e) => {
                    let why = e.to_string();
                    if why != trouble {
                        eprintln!("cannot reach {}:{}: {why}; trying again every {}s", a.host, a.port, RETRY_EVERY.as_secs());
                        trouble = why;
                    }
                    next_try = Instant::now() + RETRY_EVERY;
                }
            }
            dirty = true;
        }

        if let Some(s) = &mut session {
            let poll = last_poll.elapsed() >= POLL_EVERY;
            let noop = last_noop.elapsed() >= NOOP_EVERY;
            if poll {
                last_poll = Instant::now();
            }
            if noop {
                last_noop = Instant::now();
            }
            match exchange(s, events, t, poll, noop) {
                Ok(changed) => dirty |= changed,
                Err(e) => {
                    eprintln!("lost the Core: {e}; reconnecting");
                    last_screen = s.app.current().map(|sc| sc.component.name.clone());
                    trouble = e.to_string();
                    session = None;
                    next_try = Instant::now() + RETRY_EVERY / 2;
                    dirty = true;
                }
            }
        }

        if dirty {
            match &session {
                Some(s) => render(&s.app, &mut canvas),
                None => status(&mut canvas, "Connecting…", &format!("{}:{} — {trouble}", a.host, a.port)),
            }
            frame = canvas.to_png();
            version += 1;
            dirty = false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
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
