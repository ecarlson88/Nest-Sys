//! Running the screen against a Core, whatever shows it: connecting (and
//! trying again every RETRY_EVERY while the Core cannot be reached),
//! discovering the design, sending what the ring and click do, following
//! the Core's changes, keeping the clock and the settings, and drawing.
//!
//! A front end (the simulator's browser page, the unit's framebuffer) feeds
//! in the ring's turns and the click's presses and shows [`Runner::canvas`]
//! whenever [`Runner::step`] says it holds a new frame.

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use chrono::{Datelike, Timelike};
use qsys::{Client, Qrc, Transport, DEFAULT_PORT, PREFIX};
use serde_json::Value;

use crate::app::{App, Clock, Set, Settings};
use crate::canvas::{Canvas, FACE};
use crate::input::{Button, Event};
use crate::render::{status, Renderer};

const POLL_EVERY: Duration = Duration::from_millis(200);
const NOOP_EVERY: Duration = Duration::from_secs(20);
const RETRY_EVERY: Duration = Duration::from_secs(2);

/// Where the Core is and how to reach it, and where settings are kept.
#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: String,
    pub prefix: String,
    /// The file Settings are kept in.
    pub settings: String,
}

impl Config {
    pub fn new(host: &str, settings: &str) -> Config {
        Config {
            host: host.to_owned(),
            port: DEFAULT_PORT,
            user: None,
            password: String::new(),
            prefix: PREFIX.to_owned(),
            settings: settings.to_owned(),
        }
    }

    /// Takes one command-line option shared by every front end (--port,
    /// --user, --password, --prefix, --settings) and its value; false if
    /// `arg` is not one of them.
    pub fn option(&mut self, arg: &str, value: &mut dyn FnMut() -> Result<String, String>) -> Result<bool, String> {
        match arg {
            "--port" => self.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--user" => self.user = Some(value()?),
            "--password" => self.password = value()?,
            "--prefix" => self.prefix = value()?,
            "--settings" => self.settings = value()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// What the ring and click did, as a front end reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// Detents turned, positive clockwise.
    Turn(i32),
    Down,
    Up,
}

/// A connection to the Core and the screen built from what it reported.
struct Session {
    q: Qrc<TcpStream>,
    app: App,
    /// The Core's last EngineStatus params (design name, state, ...).
    engine: Value,
}

/// What the Network page shows.
fn network_rows(c: &Config, s: &Session) -> Vec<(String, String)> {
    let mut rows = vec![("Core".to_owned(), format!("{}:{}", c.host, c.port))];
    if let Ok(local) = s.q.client.get_ref().local_addr() {
        rows.push(("This unit".into(), local.ip().to_string()));
    }
    if let Some(design) = s.engine.get("DesignName").and_then(Value::as_str) {
        let emulated = s.engine.get("IsEmulator").and_then(Value::as_bool) == Some(true);
        rows.push(("Design".into(), if emulated { format!("{design} (emulated)") } else { design.to_owned() }));
    }
    if let Some(state) = s.engine.get("State").and_then(Value::as_str) {
        rows.push(("Core state".into(), state.to_owned()));
    }
    rows
}

/// Takes the Core's unasked messages, keeping its EngineStatus; returns
/// whether that changed.
fn take_engine_status(s: &mut Session) -> bool {
    let mut changed = false;
    for m in s.q.client.take_notifications() {
        if m.get("method").and_then(Value::as_str) == Some("EngineStatus") {
            s.engine = m.get("params").cloned().unwrap_or(Value::Null);
            changed = true;
        }
    }
    changed
}

/// The local time as the home screen shows it.
pub fn clock_now() -> Clock {
    let now = chrono::Local::now();
    Clock {
        hour: now.hour(),
        minute: now.minute(),
        date: format!("{} {} {}", now.format("%A"), now.day(), now.format("%B")),
    }
}

pub fn load_settings(path: &str) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .map_or_else(Settings::default, |v| Settings::from_json(&v))
}

pub fn save_settings(path: &str, settings: &Settings) {
    if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(&settings.to_json()).unwrap_or_default();
    if let Err(e) = std::fs::write(path, text + "\n") {
        eprintln!("{path}: {e}");
    }
}

/// Connects, discovers the design and follows its controls.
fn connect(c: &Config) -> qsys::Result<Session> {
    let mut q = Qrc::new(Client::connect(&c.host, c.port, Duration::from_secs(3))?);
    if let Some(user) = &c.user {
        q.client.logon(user, &c.password)?;
    }
    let app = App::new(q.list(&c.prefix)?, &c.prefix);
    for (component, controls) in app.watched() {
        let names: Vec<&str> = controls.iter().map(String::as_str).collect();
        q.subscribe(&component, &names)?;
    }
    let mut s = Session { q, app, engine: Value::Null };
    take_engine_status(&mut s);
    s.app.network = network_rows(c, &s);
    Ok(s)
}

/// Whether an error means the connection is gone (rather than the Core
/// refusing one request).
fn lost(e: &qsys::Error) -> bool {
    !matches!(e, qsys::Error::Rpc { .. })
}

/// One round with the Core: the inputs, the values to send, the changes.
fn exchange(s: &mut Session, events: Vec<Event>, t: u64, poll: bool, noop: bool) -> qsys::Result<bool> {
    let mut dirty = s.app.tick(t);
    // A dial's turns are sent as one value per round.
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
    Ok(dirty)
}

pub struct Runner {
    config: Config,
    start: Instant,
    button: Button,
    events: Vec<Event>,
    session: Option<Session>,
    /// The screen to come back to after a reconnect, by component.
    last_screen: Option<String>,
    next_try: Instant,
    trouble: String,
    last_poll: Instant,
    last_noop: Instant,
    saved: Settings,
    renderer: Renderer,
    dirty: bool,
    animating: bool,
    /// The face as last drawn.
    pub canvas: Canvas,
}

impl Runner {
    pub fn new(config: Config) -> Runner {
        let saved = load_settings(&config.settings);
        let now = Instant::now();
        Runner {
            config,
            start: now,
            button: Button::default(),
            events: Vec::new(),
            session: None,
            last_screen: None,
            next_try: now,
            trouble: "Starting".into(),
            last_poll: now,
            last_noop: now,
            saved,
            renderer: Renderer::new(),
            dirty: true,
            animating: false,
            canvas: Canvas::new(FACE),
        }
    }

    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// What the ring or click just did.
    pub fn input(&mut self, input: Input) {
        let t = self.now_ms();
        match input {
            Input::Turn(n) => self.events.push(Event::Turn(n)),
            Input::Down => self.events.extend(self.button.down(t)),
            Input::Up => self.events.extend(self.button.up(t)),
        }
    }

    /// Whether something is moving on screen, so step is wanted again soon.
    pub fn animating(&self) -> bool {
        self.animating
    }

    /// A round of work: connects if need be, acts on the inputs since the
    /// last step, talks to the Core and draws. Returns whether `canvas`
    /// holds a new frame to show.
    pub fn step(&mut self) -> bool {
        let t = self.now_ms();
        let mut events = std::mem::take(&mut self.events);
        events.extend(self.button.tick(t));

        if self.session.is_none() && Instant::now() >= self.next_try {
            self.try_connect();
        }

        if let Some(s) = &mut self.session {
            let poll = self.last_poll.elapsed() >= POLL_EVERY;
            let noop = self.last_noop.elapsed() >= NOOP_EVERY;
            if poll {
                self.last_poll = Instant::now();
            }
            if noop {
                self.last_noop = Instant::now();
            }
            let clock = clock_now();
            if s.app.clock != clock {
                s.app.clock = clock;
                self.dirty |= s.app.idle;
            }
            match exchange(s, events, t, poll, noop) {
                Ok(changed) => {
                    self.dirty |= changed;
                    if take_engine_status(s) {
                        s.app.network = network_rows(&self.config, s);
                        self.dirty = true;
                    }
                    if s.app.settings != self.saved {
                        self.saved = s.app.settings.clone();
                        save_settings(&self.config.settings, &self.saved);
                    }
                }
                Err(e) => {
                    eprintln!("lost the Core: {e}; reconnecting");
                    self.last_screen = s.app.current().map(|sc| sc.component.name.clone());
                    self.trouble = e.to_string();
                    self.session = None;
                    self.next_try = Instant::now() + RETRY_EVERY / 2;
                    self.dirty = true;
                }
            }
        }

        if !(self.dirty || self.animating) {
            return false;
        }
        match &self.session {
            Some(s) => self.animating = self.renderer.frame(&s.app, &mut self.canvas, t),
            None => {
                self.animating = false;
                let c = &self.config;
                status(&mut self.canvas, "Connecting…", &format!("{}:{} — {}", c.host, c.port, self.trouble));
            }
        }
        self.dirty = false;
        true
    }

    fn try_connect(&mut self) {
        let c = &self.config;
        match connect(c) {
            Ok(mut s) => {
                println!("connected to {}:{}: {} screens", c.host, c.port, s.app.screens.len());
                s.app.settings = self.saved.clone();
                if let Some(name) = &self.last_screen {
                    if let Some(i) = s.app.screens.iter().position(|sc| &sc.component.name == name) {
                        s.app.screen = i;
                        s.app.focus = s.app.screens[i].stops()[0];
                    }
                }
                self.session = Some(s);
                self.last_poll = Instant::now() - POLL_EVERY;
                self.last_noop = Instant::now();
            }
            Err(e) => {
                let why = e.to_string();
                if why != self.trouble {
                    eprintln!("cannot reach {}:{}: {why}; trying again every {}s", c.host, c.port, RETRY_EVERY.as_secs());
                    self.trouble = why;
                }
                self.next_try = Instant::now() + RETRY_EVERY;
            }
        }
        self.dirty = true;
    }
}
