//! What the screen shows and how the ring and click move through it, apart
//! from Q-SYS and the display so it can be tested on its own.
//!
//! One component per screen, of two layouts:
//!
//! - **Volume**, for a gain component: the ring sets the gain as it turns,
//!   one percent of its range a detent, shown as 0-100% (while muted, it
//!   first unmutes), a click toggles its mute, and a hold
//!   opens the list of components. Its bypass and invert are not shown.
//! - **Selector**, for a component whose one control is a combo box: the
//!   ring scrolls its choices at once, a click sends the one shown, and
//!   EDIT_TIMEOUT_MS without input goes back to the current one; a hold
//!   opens the list of components.
//! - **Controls**, for any other: the ring moves between its controls and,
//!   after the last, a "Components" stop (wrapping); a click acts on the
//!   control (a toggle flips, a button fires, a dial, number or picker opens
//!   for editing) or, on the stop, opens the list of components; a hold also
//!   opens the list, except on a momentary button.
//!
//! While editing, the ring changes the value (a dial's is sent as it turns;
//! a picker's when the click picks it) and a click or hold closes it, as
//! does EDIT_TIMEOUT_MS without input (a picker's choice is then dropped).
//! A momentary button is on while the click is down, and its release is
//! always sent. The list of components ("Controls" on screen): the ring
//! moves and a click opens one. A trigger flashes for FLASH_MS when fired.
//!
//! The list of components ends with **Settings**: a list of its own (the
//! ring moves, a click acts) with the helper text switch, the idle delay,
//! the clock's format, the Network page and a way back to the list; a hold
//! there also goes back to the list.
//!
//! After the idle delay without input the screen goes **idle**, showing the
//! time; the first input after that only wakes it, back where it was.
//!
//! Controls the Core reports Invisible or Disabled are not shown, and a
//! component whose `bypass` is on is left out altogether, for as long as
//! that lasts.

use qsys::{pick_for, Change, Component, Control, Discovered, Kind, Widget};
use serde_json::{json, Value};

use crate::input::Event;

/// Editing ends after this long without a turn or a click, so the next
/// person to turn the ring does not change a value unawares.
pub const EDIT_TIMEOUT_MS: u64 = 5000;

/// How long a fired trigger shows as pressed.
pub const FLASH_MS: u64 = 400;

/// The component type Q-SYS gives a Gain.
pub const GAIN: &str = "gain";

pub struct Item {
    pub control: Control,
    pub widget: Widget,
    /// The design's Legend when it gives one, else the control's name
    /// made readable (pretty_name).
    pub label: String,
    pub invisible: bool,
    pub disabled: bool,
}

impl Item {
    fn new(control: Control, widget: Widget) -> Item {
        Item { label: control.name.clone(), control, widget, invisible: false, disabled: false }
    }

    fn named(mut self, siblings: &[&str]) -> Item {
        self.label = pretty_name(&self.control.name, siblings);
        self
    }

    pub fn shown(&self) -> bool {
        !self.invisible && !self.disabled
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Controls,
    /// Indexes into the screen's items.
    Volume { gain: usize, mute: Option<usize> },
    /// The index of the one item, a picker.
    Selector { item: usize },
}

pub struct Screen {
    pub component: Component,
    pub title: String,
    pub items: Vec<Item>,
    pub layout: Layout,
    /// Whether the component has a `bypass` control (followed, not shown).
    pub has_bypass: bool,
    pub bypassed: bool,
}

impl Screen {
    pub fn shown(&self) -> bool {
        !self.bypassed
            && match self.layout {
                Layout::Volume { gain, .. } => self.items[gain].shown(),
                Layout::Selector { item } => self.items[item].shown(),
                Layout::Controls => self.items.iter().any(Item::shown),
            }
    }

    /// Where the ring stops on a Controls screen: each shown control's
    /// index, then items.len() for the Components stop. A Selector's one
    /// stop is its item.
    pub fn stops(&self) -> Vec<usize> {
        if let Layout::Selector { item } = self.layout {
            return vec![item];
        }
        let mut stops: Vec<usize> = (0..self.items.len()).filter(|&i| self.items[i].shown()).collect();
        stops.push(self.items.len());
        stops
    }

    pub fn muted(&self) -> bool {
        match self.layout {
            Layout::Volume { mute: Some(m), .. } => self.items[m].control.is_on(),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Browse,
    /// Editing the focused control; for a picker, `choice` is the one shown.
    Edit { choice: usize },
    /// The list of components, with the one shown (SETTINGS for the
    /// Settings entry at its end).
    Menu { focus: usize },
    /// The Settings list, with the row shown (SETTINGS_ROWS).
    Settings { focus: usize },
    /// The network information page.
    Network,
}

/// The Settings entry's place in Mode::Menu.
pub const SETTINGS: usize = usize::MAX;

/// The rows of the Settings list, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsRow {
    Hints,
    Idle,
    Clock,
    Network,
    Back,
}

pub const SETTINGS_ROWS: [SettingsRow; 5] =
    [SettingsRow::Hints, SettingsRow::Idle, SettingsRow::Clock, SettingsRow::Network, SettingsRow::Back];

/// The idle delays a click on the Idle row steps through, in seconds; None
/// is never.
pub const IDLE_CHOICES: [Option<u64>; 4] = [Some(30), Some(60), Some(300), None];

/// What the user chose in Settings, kept by the caller between runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Whether the small "Click to ..." hints are shown.
    pub hints: bool,
    /// Seconds without input before the home screen; None for never.
    pub idle_s: Option<u64>,
    /// A 24-hour clock rather than 12-hour.
    pub clock24: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { hints: true, idle_s: Some(60), clock24: false }
    }
}

impl Settings {
    /// From the JSON `to_json` wrote; missing or odd fields keep their
    /// defaults.
    pub fn from_json(v: &Value) -> Settings {
        let mut s = Settings::default();
        if let Some(h) = v.get("hints").and_then(Value::as_bool) {
            s.hints = h;
        }
        match v.get("idle_s") {
            Some(Value::Null) => s.idle_s = None,
            Some(n) => s.idle_s = n.as_u64().filter(|&n| n > 0).or(s.idle_s),
            None => {}
        }
        if let Some(c) = v.get("clock24").and_then(Value::as_bool) {
            s.clock24 = c;
        }
        s
    }

    pub fn to_json(&self) -> Value {
        json!({"hints": self.hints, "idle_s": self.idle_s, "clock24": self.clock24})
    }

    /// The idle delay after this one, round IDLE_CHOICES.
    fn next_idle(&self) -> Option<u64> {
        let at = IDLE_CHOICES.iter().position(|&c| c == self.idle_s).unwrap_or(0);
        IDLE_CHOICES[(at + 1) % IDLE_CHOICES.len()]
    }
}

/// The time to show when idle, from the caller's clock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clock {
    pub hour: u32,
    pub minute: u32,
    /// The date, written out ("Thursday 13 November").
    pub date: String,
}

/// A control's name for the screen when the design gives no Legend:
/// words split at dots and underscores, capitalised ("LED" for led), and
/// an index dropped when no other control in the component shares the
/// name before it ("toggle.1" is "Toggle", unless there is a "toggle.2").
pub fn pretty_name(name: &str, siblings: &[&str]) -> String {
    let base = match name.rsplit_once('.') {
        Some((b, i)) if !i.is_empty() && i.bytes().all(|c| c.is_ascii_digit()) => Some((b, i)),
        _ => None,
    };
    let (stem, index) = match base {
        Some((b, i)) => {
            let shared = siblings.iter().filter(|s| s.rsplit_once('.').is_some_and(|(sb, _)| sb == b)).count() > 1;
            (b, shared.then_some(i))
        }
        None => (name, None),
    };
    let mut words: Vec<String> = stem
        .split(['.', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| match w.to_ascii_lowercase().as_str() {
            "led" => "LED".to_owned(),
            "text" if stem.contains("display") => String::new(),
            _ => {
                let mut c = w.chars();
                c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
            }
        })
        .filter(|w| !w.is_empty())
        .collect();
    if let Some(i) = index {
        words.push(i.to_owned());
    }
    if words.is_empty() { name.to_owned() } else { words.join(" ") }
}

/// A value to send to the Core.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub component: String,
    pub control: String,
    pub value: Value,
}

pub struct App {
    pub screens: Vec<Screen>,
    pub screen: usize,
    /// An index into the screen's items, or items.len() for the Components
    /// stop (Controls screens only).
    pub focus: usize,
    pub mode: Mode,
    /// The momentary button the click holds on: (component, control).
    pub pressed: Option<(String, String)>,
    /// When the last input came, in the caller's milliseconds.
    pub last_input_ms: u64,
    /// A trigger just fired: (component, control, until when).
    pub flash: Option<(String, String, u64)>,
    pub settings: Settings,
    /// What the Network page shows, as (label, value), from the caller.
    pub network: Vec<(String, String)>,
    /// Showing the home screen after the idle delay.
    pub idle: bool,
    /// The time, kept current by the caller.
    pub clock: Clock,
    /// The click that woke the screen, still to be let go: its release and
    /// click are not acted on.
    waking: bool,
}

/// The title of a component's screen: its Code Name without the prefix,
/// underscores as spaces.
pub fn title(name: &str, prefix: &str) -> String {
    let rest = if qsys::has_prefix(name, prefix) { &name[prefix.len()..] } else { name };
    let t = rest.replace('_', " ").trim().to_owned();
    if t.is_empty() { name.to_owned() } else { t }
}

/// A round step near a hundredth of the range: 1, 2 or 5 times a power of ten.
fn nice_step(span: f64) -> f64 {
    let raw = span / 100.0;
    if raw.is_nan() || raw <= 0.0 || raw.is_infinite() {
        return 1.0;
    }
    let e = 10f64.powf(raw.log10().floor());
    let m = raw / e;
    let n = if m < 1.5 {
        1.0
    } else if m < 3.5 {
        2.0
    } else if m < 7.5 {
        5.0
    } else {
        10.0
    };
    n * e
}

/// How far one detent moves a number.
pub fn step(c: &Control) -> f64 {
    let integer = c.type_name.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("integer"));
    let s = c.range().map_or(1.0, |(lo, hi)| nice_step(hi - lo));
    if integer { s.max(1.0).round() } else { s }
}

fn wrap(i: usize, by: i32, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (i as i64 + by as i64).rem_euclid(n as i64) as usize
}

/// Turns a number n detents, on its steps and inside its range; returns
/// the value to send.
fn turn_number(c: &mut Control, n: i32) -> Value {
    let s = step(c);
    let now = c.value.as_f64().unwrap_or(0.0);
    let mut v = ((now + n as f64 * s) / s).round() * s;
    if let Some((lo, hi)) = c.range() {
        v = v.clamp(lo, hi);
    }
    // Clean off float noise from the rounding (0.30000000000000004).
    let v = json!((v * 1e6).round() / 1e6);
    c.value = v.clone();
    c.string = None;
    v
}

/// A gain's level as a whole percentage of its range (what Q-SYS calls
/// its Position, which for a gain runs evenly in dB).
pub fn percent(c: &Control) -> Option<i64> {
    let (lo, hi) = c.range()?;
    let v = c.value.as_f64()?;
    Some((((v - lo) / (hi - lo)) * 100.0).round().clamp(0.0, 100.0) as i64)
}

/// Turns a gain n percent of its range, on whole percents; returns the
/// value to send.
fn turn_percent(c: &mut Control, n: i32) -> Value {
    let Some((lo, hi)) = c.range() else { return turn_number(c, n) };
    let pct = (percent(c).unwrap_or(0) + n as i64).clamp(0, 100);
    let v = json!(((lo + (hi - lo) * pct as f64 / 100.0) * 1e6).round() / 1e6);
    c.value = v.clone();
    c.string = None;
    v
}

fn is_bypass(c: &Control) -> bool {
    c.name == "bypass" && c.kind == Kind::Boolean
}

fn screen_of(d: Discovered, prefix: &str) -> Option<Screen> {
    let title = title(&d.component.name, prefix);
    let bypass = d.controls.iter().find(|c| is_bypass(c));
    let (has_bypass, bypassed) = (bypass.is_some(), bypass.is_some_and(Control::is_on));
    let mut gain = None;
    let mut mute = None;
    let mut items = Vec::new();
    let names: Vec<String> = d.controls.iter().map(|c| c.name.clone()).collect();
    let siblings: Vec<&str> = names.iter().map(String::as_str).collect();
    for control in d.controls.into_iter().filter(|c| !is_bypass(c)) {
        let (widget, _) = pick_for(&d.component, &control);
        if widget == Widget::Hidden {
            continue;
        }
        match (control.name.as_str(), &widget) {
            ("gain", Widget::Dial { .. }) => gain = Some(items.len()),
            ("mute", Widget::Toggle) => mute = Some(items.len()),
            _ => {}
        }
        items.push(Item::new(control, widget).named(&siblings));
    }
    let layout = match (d.component.kind.as_deref(), gain) {
        (Some(GAIN), Some(gain)) => {
            // Only the gain and its mute: the volume knob.
            let keep = |i: usize| i == gain || Some(i) == mute;
            let mut kept = Vec::new();
            let (mut g, mut m) = (0, None);
            for (i, item) in items.into_iter().enumerate().filter(|(i, _)| keep(*i)) {
                if i == gain {
                    g = kept.len();
                } else {
                    m = Some(kept.len());
                }
                kept.push(item);
            }
            items = kept;
            Layout::Volume { gain: g, mute: m }
        }
        _ if items.len() == 1 && matches!(items[0].widget, Widget::Picker { .. }) => Layout::Selector { item: 0 },
        _ => Layout::Controls,
    };
    (!items.is_empty()).then_some(Screen { component: d.component, title, items, layout, has_bypass, bypassed })
}

impl App {
    pub fn new(found: Vec<Discovered>, prefix: &str) -> App {
        let screens = found.into_iter().filter_map(|d| screen_of(d, prefix)).collect();
        let mut app = App { screens, screen: 0, focus: 0, mode: Mode::Browse, pressed: None, last_input_ms: 0, flash: None,
            settings: Settings::default(), network: Vec::new(), idle: false, clock: Clock::default(),
            waking: false };
        app.settle();
        app
    }

    /// The screen shown, if any component is shown.
    pub fn current(&self) -> Option<&Screen> {
        self.screens.get(self.screen).filter(|s| s.shown())
    }

    /// The focused control; None on the Components stop or a Volume screen.
    pub fn focused(&self) -> Option<&Item> {
        let s = self.current()?;
        match s.layout {
            Layout::Controls => s.items.get(self.focus).filter(|i| i.shown()),
            Layout::Selector { item } => s.items.get(item),
            Layout::Volume { .. } => None,
        }
    }

    /// Whether the focus is on the stop after the last control that opens
    /// the list of components.
    pub fn on_components_stop(&self) -> bool {
        self.mode == Mode::Browse
            && self.current().is_some_and(|s| s.layout == Layout::Controls && self.focus == s.items.len())
    }

    /// The entries of the list of components: the shown screens' indices,
    /// then SETTINGS.
    pub fn menu_entries(&self) -> Vec<usize> {
        let mut entries = self.shown_screens();
        entries.push(SETTINGS);
        entries
    }

    /// The screens that are shown, by index.
    pub fn shown_screens(&self) -> Vec<usize> {
        (0..self.screens.len()).filter(|&i| self.screens[i].shown()).collect()
    }

    /// The controls to follow, by component: every one shown or that can
    /// come to be, and each component's bypass.
    pub fn watched(&self) -> Vec<(String, Vec<String>)> {
        self.screens
            .iter()
            .map(|s| {
                let mut names: Vec<String> = s.items.iter().map(|i| i.control.name.clone()).collect();
                if s.has_bypass {
                    names.push("bypass".into());
                }
                (s.component.name.clone(), names)
            })
            .collect()
    }

    /// Time passing: ends editing after EDIT_TIMEOUT_MS without input.
    /// Returns whether anything changed on screen.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        let quiet = now_ms.saturating_sub(self.last_input_ms);
        if !self.idle && self.pressed.is_none() && self.settings.idle_s.is_some_and(|s| quiet >= s * 1000) {
            self.idle = true;
            if matches!(self.mode, Mode::Edit { .. }) {
                self.mode = Mode::Browse;
            }
            return true;
        }
        if self.flash.as_ref().is_some_and(|f| now_ms >= f.2) {
            self.flash = None;
            return true;
        }
        if matches!(self.mode, Mode::Edit { .. }) && now_ms.saturating_sub(self.last_input_ms) >= EDIT_TIMEOUT_MS {
            self.mode = Mode::Browse;
            return true;
        }
        false
    }

    /// Puts the screen, focus and mode back on something shown, after the
    /// Core hid or bypassed what they were on.
    fn settle(&mut self) {
        let shown = self.shown_screens();
        if let Mode::Menu { focus } = self.mode {
            if focus != SETTINGS && !shown.contains(&focus) {
                self.mode = Mode::Menu { focus: shown.first().copied().unwrap_or(SETTINGS) };
            }
        }
        let Some(&first) = shown.first() else {
            if matches!(self.mode, Mode::Edit { .. }) {
                self.mode = Mode::Browse;
            }
            return;
        };
        if !shown.contains(&self.screen) {
            self.screen = first;
            self.focus = 0;
            if matches!(self.mode, Mode::Edit { .. }) {
                self.mode = Mode::Browse;
            }
        }
        let s = &self.screens[self.screen];
        if let Layout::Selector { item } = s.layout {
            self.focus = item;
        }
        if s.layout == Layout::Controls {
            let stops = s.stops();
            if !stops.contains(&self.focus) {
                // The next stop after the one hidden.
                self.focus = stops.iter().copied().find(|&i| i > self.focus).unwrap_or(stops[0]);
                if matches!(self.mode, Mode::Edit { .. }) {
                    self.mode = Mode::Browse;
                }
            }
        }
    }

    /// An input at now_ms (the clock tick() is given).
    pub fn handle(&mut self, ev: Event, now_ms: u64) -> Vec<Set> {
        self.last_input_ms = now_ms;
        // Idle, the first input only wakes the screen; a click that woke
        // it is not acted on when let go.
        if self.idle {
            self.idle = false;
            self.waking = ev == Event::Down;
            return Vec::new();
        }
        if self.waking {
            match ev {
                Event::Up => return Vec::new(),
                Event::Click | Event::Hold => {
                    if ev == Event::Click {
                        self.waking = false;
                    }
                    return Vec::new();
                }
                _ => self.waking = false,
            }
        }
        // A momentary button goes off when the click lets go, wherever the
        // screen has got to; while it is held, a hold means nothing.
        if let Some((component, control)) = self.pressed.clone() {
            match ev {
                Event::Up => {
                    self.pressed = None;
                    self.set_local(&component, &control, json!(false));
                    return vec![Set { component, control, value: json!(false) }];
                }
                Event::Hold => return Vec::new(),
                _ => {}
            }
        }
        match self.mode {
            Mode::Menu { focus } => {
                let entries = self.menu_entries();
                match ev {
                    Event::Turn(n) => {
                        let at = entries.iter().position(|&i| i == focus).unwrap_or(0);
                        self.mode = Mode::Menu { focus: entries[wrap(at, n, entries.len())] };
                    }
                    Event::Click if focus == SETTINGS => self.mode = Mode::Settings { focus: 0 },
                    Event::Click => {
                        self.screen = focus;
                        self.focus = self.screens[focus].stops()[0];
                        self.mode = Mode::Browse;
                    }
                    _ => {}
                }
                return Vec::new();
            }
            Mode::Settings { focus } => {
                match ev {
                    Event::Turn(n) => self.mode = Mode::Settings { focus: wrap(focus, n, SETTINGS_ROWS.len()) },
                    Event::Click => match SETTINGS_ROWS[focus] {
                        SettingsRow::Hints => self.settings.hints = !self.settings.hints,
                        SettingsRow::Idle => self.settings.idle_s = self.settings.next_idle(),
                        SettingsRow::Clock => self.settings.clock24 = !self.settings.clock24,
                        SettingsRow::Network => self.mode = Mode::Network,
                        SettingsRow::Back => self.mode = Mode::Menu { focus: SETTINGS },
                    },
                    Event::Hold => self.mode = Mode::Menu { focus: SETTINGS },
                    _ => {}
                }
                return Vec::new();
            }
            Mode::Network => {
                if matches!(ev, Event::Click | Event::Hold) {
                    let row = SETTINGS_ROWS.iter().position(|&r| r == SettingsRow::Network).unwrap_or(0);
                    self.mode = Mode::Settings { focus: row };
                }
                return Vec::new();
            }
            _ => {}
        }
        if self.current().is_none() {
            // Nothing to show but the list, whose Settings stay reachable.
            if ev == Event::Hold {
                self.mode = Mode::Menu { focus: SETTINGS };
            }
            return Vec::new();
        }
        match self.mode {
            Mode::Menu { .. } | Mode::Settings { .. } | Mode::Network => Vec::new(),
            Mode::Edit { choice } => self.edit(ev, choice),
            Mode::Browse => match self.screens[self.screen].layout {
                Layout::Volume { gain, mute } => self.volume(ev, gain, mute),
                Layout::Selector { item } => self.selector(ev, item),
                Layout::Controls => self.browse(ev),
            },
        }
    }

    fn volume(&mut self, ev: Event, gain: usize, mute: Option<usize>) -> Vec<Set> {
        let screen = &mut self.screens[self.screen];
        let component = screen.component.name.clone();
        match ev {
            // While muted, the first turn unmutes and leaves the level.
            Event::Turn(_) if screen.muted() => {
                let m = mute.expect("muted means a mute");
                let c = &mut screen.items[m].control;
                c.value = json!(false);
                c.string = None;
                vec![Set { component, control: c.name.clone(), value: json!(false) }]
            }
            Event::Turn(n) => {
                let c = &mut screen.items[gain].control;
                let value = turn_percent(c, n);
                vec![Set { component, control: c.name.clone(), value }]
            }
            Event::Click => {
                let Some(m) = mute.filter(|&m| screen.items[m].shown()) else { return Vec::new() };
                let c = &mut screen.items[m].control;
                let on = !c.is_on();
                c.value = json!(on);
                c.string = None;
                vec![Set { component, control: c.name.clone(), value: json!(on) }]
            }
            Event::Hold => {
                self.mode = Mode::Menu { focus: self.screen };
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A Selector at rest: a turn starts scrolling its choices from the
    /// current one (the scrolling and picking are a picker's editing).
    fn selector(&mut self, ev: Event, item: usize) -> Vec<Set> {
        match ev {
            Event::Turn(n) => {
                let c = &self.screens[self.screen].items[item];
                let Widget::Picker { choices } = &c.widget else { return Vec::new() };
                let current = c.control.string.as_deref().unwrap_or("");
                let at = choices.iter().position(|ch| ch == current).unwrap_or(0);
                self.focus = item;
                self.mode = Mode::Edit { choice: wrap(at, n, choices.len()) };
            }
            Event::Hold => self.mode = Mode::Menu { focus: self.screen },
            _ => {}
        }
        Vec::new()
    }

    fn browse(&mut self, ev: Event) -> Vec<Set> {
        let component = self.screens[self.screen].component.name.clone();
        let stops = self.screens[self.screen].stops();
        match ev {
            Event::Turn(n) => {
                let at = stops.iter().position(|&i| i == self.focus).unwrap_or(0);
                self.focus = stops[wrap(at, n, stops.len())];
                return Vec::new();
            }
            Event::Hold => {
                self.mode = Mode::Menu { focus: self.screen };
                return Vec::new();
            }
            _ => {}
        }
        let Some(item) = self.screens[self.screen].items.get_mut(self.focus) else {
            if ev == Event::Click {
                self.mode = Mode::Menu { focus: self.screen };
            }
            return Vec::new();
        };
        let set = |item: &Item, value: Value| {
            vec![Set { component: component.clone(), control: item.control.name.clone(), value }]
        };
        match (ev, &item.widget) {
            (Event::Down, Widget::Momentary) => {
                item.control.value = json!(true);
                item.control.string = None;
                self.pressed = Some((component.clone(), item.control.name.clone()));
                set(item, json!(true))
            }
            (Event::Click, Widget::Toggle) => {
                let on = !item.control.is_on();
                item.control.value = json!(on);
                item.control.string = None;
                set(item, json!(on))
            }
            // Any value fires a trigger.
            (Event::Click, Widget::Button) => {
                self.flash = Some((component.clone(), item.control.name.clone(), self.last_input_ms + FLASH_MS));
                set(item, json!(1))
            }
            (Event::Click, Widget::Dial { .. } | Widget::NumberEntry) => {
                self.mode = Mode::Edit { choice: 0 };
                Vec::new()
            }
            (Event::Click, Widget::Picker { choices }) => {
                let current = item.control.string.as_deref().unwrap_or("");
                let choice = choices.iter().position(|c| c == current).unwrap_or(0);
                self.mode = Mode::Edit { choice };
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn edit(&mut self, ev: Event, choice: usize) -> Vec<Set> {
        let component = self.screens[self.screen].component.name.clone();
        let Some(item) = self.screens[self.screen].items.get_mut(self.focus) else {
            self.mode = Mode::Browse;
            return Vec::new();
        };
        match (ev, item.widget.clone()) {
            (Event::Click | Event::Hold, Widget::Picker { choices }) => {
                self.mode = Mode::Browse;
                if ev == Event::Hold {
                    // A Selector's hold drops the choice and opens the list.
                    if matches!(self.screens[self.screen].layout, Layout::Selector { .. }) {
                        self.mode = Mode::Menu { focus: self.screen };
                    }
                    return Vec::new();
                }
                let Some(pick) = choices.get(choice) else { return Vec::new() };
                item.control.string = Some(pick.clone());
                item.control.value = json!(pick);
                vec![Set { component, control: item.control.name.clone(), value: json!(pick) }]
            }
            (Event::Click | Event::Hold, _) => {
                self.mode = Mode::Browse;
                Vec::new()
            }
            (Event::Turn(n), Widget::Picker { choices }) => {
                self.mode = Mode::Edit { choice: wrap(choice, n, choices.len()) };
                Vec::new()
            }
            (Event::Turn(n), Widget::Dial { .. } | Widget::NumberEntry) => {
                let value = turn_number(&mut item.control, n);
                vec![Set { component, control: item.control.name.clone(), value }]
            }
            _ => Vec::new(),
        }
    }

    fn set_local(&mut self, component: &str, control: &str, value: Value) {
        for screen in self.screens.iter_mut().filter(|s| s.component.name == component) {
            for item in screen.items.iter_mut().filter(|i| i.control.name == control) {
                item.control.value = value.clone();
                item.control.string = None;
            }
        }
    }

    /// A change the Core reported.
    pub fn apply(&mut self, ch: &Change) {
        let Some(component) = ch.component.as_deref() else { return };
        for screen in self.screens.iter_mut().filter(|s| s.component.name == component) {
            if screen.has_bypass && ch.name == "bypass" {
                screen.bypassed = ch.is_on();
            }
            for item in screen.items.iter_mut().filter(|i| i.control.name == ch.name) {
                item.invisible = ch.invisible;
                item.disabled = ch.disabled;
                let c = &mut item.control;
                c.string = ch.string.clone();
                c.value = match (&c.kind, &ch.string) {
                    // Get and Poll give Text a meaningless 0.0 Value.
                    (Kind::Text, Some(s)) => json!(s),
                    _ => ch.value.clone(),
                };
                if let Some(choices) = ch.choices.as_ref().filter(|c| !c.is_empty()) {
                    if c.choices.as_ref() != Some(choices) {
                        c.choices = Some(choices.clone());
                        item.widget = pick_for(&screen.component, c).0;
                    }
                }
                if let Some(legend) = &ch.legend {
                    item.label = legend.clone();
                }
            }
        }
        self.settle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_by_a_round_hundredth() {
        assert_eq!(nice_step(120.0), 1.0);
        assert_eq!(nice_step(140.0), 1.0);
        assert_eq!(nice_step(1.0), 0.01);
        assert_eq!(nice_step(300.0), 2.0);
        assert_eq!(nice_step(0.0), 1.0);
    }

    #[test]
    fn titles_without_the_prefix() {
        assert_eq!(title("NEST_Zone1", "NEST_"), "Zone1");
        assert_eq!(title("Nest_Combo", "NEST_"), "Combo");
        assert_eq!(title("NEST_Lobby_Music", "NEST_"), "Lobby Music");
        assert_eq!(title("NEST_", "NEST_"), "NEST_");
    }
}
