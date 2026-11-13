//! The screen against a real Core or Designer's emulation, shown in a
//! browser in place of the unit's display: the mouse wheel or the arrow
//! keys turn the ring, a mouse button or the space bar is the click.
//!
//!     cargo run -p surface --bin sim -- CORE_HOST [--port 1710]
//!         [--user U --password P] [--prefix NEST_] [--listen 127.0.0.1:8480]
//!         [--settings surface-settings.json]
//!
//! Settings chosen on the screen are kept in the --settings file.
//!
//! then open http://127.0.0.1:8480. When the Core goes away (Designer
//! leaving emulation, say), it shows "Connecting…" and tries again every
//! two seconds, discovering the design afresh when it is back.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use surface::runner::{Config, Input, Runner};

const PAGE: &str = include_str!("sim.html");

struct Args {
    config: Config,
    listen: String,
}

fn parse() -> Result<Args, String> {
    let mut a = Args { config: Config::new("", "surface-settings.json"), listen: "127.0.0.1:8480".into() };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        if a.config.option(&arg, &mut value)? {
            continue;
        }
        match arg.as_str() {
            "--listen" => a.listen = value()?,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => a.config.host = s.into(),
        }
    }
    if a.config.host.is_empty() {
        return Err("usage: sim CORE_HOST [--port N] [--user U --password P] [--prefix P] [--listen ADDR] \
                    [--settings FILE]"
            .into());
    }
    Ok(a)
}

/// What the browser asked for.
enum Request {
    Page,
    /// The latest frame, unless the browser already has version `have`.
    Frame(Option<u64>),
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
        "/frame" => Request::Frame(param("have").and_then(|v| v.parse().ok())),
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

fn respond_frame(stream: &mut TcpStream, version: u64, png: &[u8]) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nX-Version: {version}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        png.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(png);
}

fn run(a: Args) -> std::io::Result<()> {
    let listener = TcpListener::bind(&a.listen)?;
    listener.set_nonblocking(true)?;
    println!("open http://{}", a.listen);
    let mut runner = Runner::new(a.config);
    let mut frame = Vec::new();
    let mut version = 0u64;
    loop {
        while let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(false);
            match read_request(&mut stream) {
                Some(Request::Page) => respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes()),
                Some(Request::Frame(have)) if have == Some(version) => {
                    respond(&mut stream, "204 No Content", "text/plain", b"")
                }
                Some(Request::Frame(_)) => respond_frame(&mut stream, version, &frame),
                Some(Request::Input(e, n)) => {
                    match e.as_str() {
                        "turn" => runner.input(Input::Turn(n)),
                        "down" => runner.input(Input::Down),
                        "up" => runner.input(Input::Up),
                        _ => {}
                    }
                    respond(&mut stream, "204 No Content", "text/plain", b"");
                }
                _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
            }
        }
        if runner.step() {
            frame = runner.canvas.to_png();
            version += 1;
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
