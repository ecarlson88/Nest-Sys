//! Running the screen against a Core, whatever shows it: connecting (on a
//! thread of its own, and trying again every RETRY_EVERY while the Core
//! cannot be reached), then talking to it from another thread (sending what
//! the ring sets, polling, keeping the connection alive), so that the ring
//! and the screen never wait on the network,
//! discovering the design, sending what the ring and click do, following
//! the Core's changes, keeping the clock and the settings, and drawing.
//!
//! A front end (the simulator's browser page, the unit's framebuffer) feeds
//! in the ring's turns and the click's presses and shows [`Runner::canvas`]
//! whenever [`Runner::step`] says it holds a new frame.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

use chrono::{Datelike, Timelike};
use qsys::{Change, Client, Qrc, Transport, DEFAULT_PORT, PREFIX};
use serde_json::Value;

use crate::app::{App, Clock, Set, Settings};
use crate::canvas::{Canvas, FACE};
use crate::input::{Button, Event, ListGear};
use crate::render::{status, Renderer};

const POLL_EVERY: Duration = Duration::from_millis(200);
const NOOP_EVERY: Duration = Duration::from_secs(20);
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// For this long after the ring sets a control, the Core's reports of it
/// are not shown: they can be of a value set before, still on its way
/// (the unit's Wi-Fi round trip alone can be 100 ms), and would pull the
/// level back while it turns.
const ECHO_MS: u64 = 800;

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

/// What the Core's thread reports to the main loop.
enum Report {
    Changes(Vec<Change>),
    /// The Core's EngineStatus params (design name, state, ...).
    Engine(Value),
    /// The connection is gone, and why; the thread has ended.
    Lost(String),
}

/// A connection to the Core and the screen built from what it reported.
/// The connection itself belongs to its thread (`talk`).
struct Session {
    app: App,
    /// The Core's last EngineStatus params (design name, state, ...).
    engine: Value,
    /// This unit's address on the connection, for the Network page.
    local: Option<IpAddr>,
    /// Turns to steps where the ring moves through a list.
    gear: ListGear,
    /// Values for the Core's thread to send.
    sets: Sender<Vec<Set>>,
    reports: Receiver<Report>,
    /// When the ring last set each control, by (component, control).
    recent: HashMap<(String, String), u64>,
}

/// What the Network page shows.
fn network_rows(c: &Config, s: &Session) -> Vec<(String, String)> {
    let mut rows = vec![("Core".to_owned(), format!("{}:{}", c.host, c.port))];
    if let Some(local) = s.local {
        rows.push(("This unit".into(), local.to_string()));
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

/// The latest EngineStatus among the Core's unasked messages, if any.
fn engine_status(q: &mut Qrc<TcpStream>) -> Option<Value> {
    q.client
        .take_notifications()
        .into_iter()
        .filter(|m| m.get("method").and_then(Value::as_str) == Some("EngineStatus"))
        .map(|m| m.get("params").cloned().unwrap_or(Value::Null))
        .next_back()
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

/// Connects, discovers the design and follows its controls, then hands
/// the connection to a thread of its own.
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
    let engine = engine_status(&mut q).unwrap_or(Value::Null);
    let local = q.client.get_ref().local_addr().ok().map(|a| a.ip());
    let (sets, to_send) = mpsc::channel();
    let (report, reports) = mpsc::channel();
    std::thread::spawn(move || talk(q, to_send, report));
    let mut s = Session { app, engine, local, gear: ListGear::default(), sets, reports, recent: HashMap::new() };
    s.app.network = network_rows(c, &s);
    Ok(s)
}

/// Whether an error means the connection is gone (rather than the Core
/// refusing one request).
fn lost(e: &qsys::Error) -> bool {
    !matches!(e, qsys::Error::Rpc { .. })
}

/// The Core's thread: sends the values it is given (the latest of each
/// control, when several wait), polls every POLL_EVERY and keeps the
/// connection alive, reporting what comes back. Ends when the connection
/// is lost, or when the session is dropped.
fn talk(mut q: Qrc<TcpStream>, to_send: Receiver<Vec<Set>>, report: Sender<Report>) {
    let mut last_poll = Instant::now() - POLL_EVERY;
    let mut last_noop = Instant::now();
    let result: qsys::Result<()> = (|| loop {
        let mut pending: BTreeMap<(String, String), Set> = BTreeMap::new();
        match to_send.recv_timeout(POLL_EVERY.saturating_sub(last_poll.elapsed())) {
            Ok(sets) => {
                for set in sets.into_iter().chain(to_send.try_iter().flatten()) {
                    pending.insert((set.component.clone(), set.control.clone()), set);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        for set in pending.into_values() {
            if let Err(e) = q.set(&set.component, &set.control, set.value) {
                if lost(&e) {
                    return Err(e);
                }
                eprintln!("{}.{}: {e}", set.component, set.control);
            }
        }
        if last_poll.elapsed() >= POLL_EVERY {
            last_poll = Instant::now();
            let changes = q.poll()?;
            if !changes.is_empty() && report.send(Report::Changes(changes)).is_err() {
                return Ok(());
            }
        }
        if last_noop.elapsed() >= NOOP_EVERY {
            last_noop = Instant::now();
            q.client.noop()?;
        }
        if let Some(engine) = engine_status(&mut q) {
            if report.send(Report::Engine(engine)).is_err() {
                return Ok(());
            }
        }
    })();
    if let Err(e) = result {
        let _ = report.send(Report::Lost(e.to_string()));
    }
}

/// One round: the inputs, the values they set (handed to the Core's
/// thread), and what that thread reported. Err if the Core is lost.
fn exchange(c: &Config, s: &mut Session, events: Vec<Event>, t: u64) -> Result<bool, String> {
    let mut dirty = s.app.tick(t);
    // A dial's turns are sent as one value per round.
    let mut sets: BTreeMap<(String, String), Set> = BTreeMap::new();
    for mut ev in events {
        match ev {
            Event::Down => s.gear.clear(),
            Event::Turn(n) if s.app.turns_scroll_a_list() => match s.gear.feed(n) {
                0 => {
                    dirty = true;
                    continue;
                }
                steps => ev = Event::Turn(steps),
            },
            _ => {}
        }
        for set in s.app.handle(ev, t) {
            sets.insert((set.component.clone(), set.control.clone()), set);
        }
        dirty = true;
    }
    if !sets.is_empty() {
        for key in sets.keys() {
            s.recent.insert(key.clone(), t);
        }
        if s.sets.send(sets.into_values().collect()).is_err() {
            return Err("the connection to the Core ended".into());
        }
    }
    s.recent.retain(|_, at| t.saturating_sub(*at) < ECHO_MS);
    loop {
        match s.reports.try_recv() {
            Ok(Report::Changes(changes)) => {
                for ch in changes {
                    let key = (ch.component.clone().unwrap_or_default(), ch.name.clone());
                    if !s.recent.contains_key(&key) {
                        s.app.apply(&ch);
                        dirty = true;
                    }
                }
            }
            Ok(Report::Engine(engine)) => {
                s.engine = engine;
                s.app.network = network_rows(c, s);
                dirty = true;
            }
            Ok(Report::Lost(why)) => return Err(why),
            Err(TryRecvError::Empty) => return Ok(dirty),
            Err(TryRecvError::Disconnected) => return Err("the connection to the Core ended".into()),
        }
    }
}

pub struct Runner {
    config: Config,
    start: Instant,
    button: Button,
    events: Vec<Event>,
    /// When the ring or click was last used, for the backlight while no
    /// session is up (the app keeps its own).
    last_input_ms: u64,
    session: Option<Session>,
    /// A connection being made, on its own thread.
    connecting: Option<Receiver<qsys::Result<Session>>>,
    /// The screen to come back to after a reconnect, by component.
    last_screen: Option<String>,
    next_try: Instant,
    trouble: String,
    saved: Settings,
    renderer: Renderer,
    dirty: bool,
    animating: bool,
    /// A long hold asked to leave (on the Controls list, or while no
    /// session is up); the front end stops when it sees it.
    quit: bool,
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
            last_input_ms: 0,
            session: None,
            connecting: None,
            last_screen: None,
            next_try: now,
            trouble: "Starting".into(),
            saved,
            renderer: Renderer::new(),
            dirty: true,
            animating: false,
            quit: false,
            canvas: Canvas::new(FACE),
        }
    }

    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// What the ring or click just did.
    pub fn input(&mut self, input: Input) {
        let t = self.now_ms();
        self.last_input_ms = t;
        match input {
            Input::Turn(n) => self.events.push(Event::Turn(n)),
            Input::Down => self.events.extend(self.button.down(t)),
            Input::Up => self.events.extend(self.button.up(t)),
        }
    }

    /// Whether a session with the Core is up.
    pub fn connected(&self) -> bool {
        self.session.is_some()
    }

    /// The screen's state, while connected.
    pub fn app(&self) -> Option<&App> {
        self.session.as_ref().map(|s| &s.app)
    }

    /// The backlight now, as a part of its full brightness (app::App::
    /// backlight). Without a session, the chosen brightness until the
    /// screen-off delay passes.
    pub fn backlight(&self) -> f32 {
        let t = self.now_ms();
        match &self.session {
            Some(s) => s.app.backlight(t),
            None if self.saved.screen_off(t.saturating_sub(self.last_input_ms)) => 0.0,
            None => self.saved.brightness as f32 / 100.0,
        }
    }

    /// The sounds the app left since last taken (app::Sound), for a front
    /// end that can play them.
    pub fn take_sounds(&mut self) -> Vec<crate::app::Sound> {
        self.session.as_mut().map(|s| s.app.take_sounds()).unwrap_or_default()
    }

    /// Draws the frame to leave on the display when leaving by a long hold,
    /// until the thermostat draws over it.
    pub fn draw_exiting(&mut self) {
        status(&mut self.canvas, "Exiting…", "Back to the thermostat");
    }

    /// Whether a long hold asked to leave the surface (front ends that can
    /// stop, the unit's, do; the screen goes back to what was there).
    pub fn quit(&self) -> bool {
        self.quit
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

        if self.session.is_none() {
            // Nothing to show but "Connecting…": a long hold leaves.
            self.quit |= events.contains(&Event::LongHold);
            self.connect_step();
        }

        if let Some(s) = &mut self.session {
            let clock = clock_now();
            if s.app.clock != clock {
                s.app.clock = clock;
                self.dirty |= s.app.idle;
            }
            match exchange(&self.config, s, events, t) {
                Ok(changed) => {
                    self.dirty |= changed;
                    self.quit |= s.app.quit;
                    if s.app.settings != self.saved {
                        self.saved = s.app.settings.clone();
                        save_settings(&self.config.settings, &self.saved);
                    }
                }
                Err(e) => {
                    eprintln!("lost the Core: {e}; reconnecting");
                    self.last_screen = s.app.current().map(|sc| sc.component.name.clone());
                    self.trouble = e;
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

    /// Starts connecting when it is time to, and takes the outcome once
    /// the thread has it; neither waits.
    fn connect_step(&mut self) {
        let outcome = match &self.connecting {
            None if Instant::now() >= self.next_try => {
                let (tx, rx) = mpsc::channel();
                let config = self.config.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(connect(&config));
                });
                self.connecting = Some(rx);
                return;
            }
            None => return,
            Some(rx) => match rx.try_recv() {
                Ok(outcome) => outcome,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err(std::io::Error::other("connecting failed unexpectedly").into()),
            },
        };
        self.connecting = None;
        self.take_session(outcome);
    }

    fn take_session(&mut self, outcome: qsys::Result<Session>) {
        let c = &self.config;
        match outcome {
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
