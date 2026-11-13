//! What the screen shows and how the ring and click move through it, apart
//! from Q-SYS and the display so it can be tested on its own.
//!
//! One component per screen. On a screen:
//! - the ring moves between its controls (wrapping);
//! - a click acts on the control: a toggle flips, a button fires, a dial,
//!   number or picker opens for editing;
//! - holding the click opens the list of components.
//!
//! While editing, the ring changes the value (a dial's is sent as it turns;
//! a picker's when the click picks it) and a click or hold closes it. A
//! momentary button is on while the click is down; holding it does not open
//! the list, and its release is always sent. The list of components: the
//! ring moves, a click opens one, a hold goes back.

use qsys::{pick_for, Change, Component, Control, Discovered, Kind, Widget};
use serde_json::{json, Value};

use crate::input::Event;

pub struct Item {
    pub control: Control,
    pub widget: Widget,
    /// The design's Legend when it gives one, else the control's name.
    pub label: String,
}

pub struct Screen {
    pub component: Component,
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Browse,
    /// Editing the focused control; for a picker, `choice` is the one shown.
    Edit { choice: usize },
    /// The list of components, with the one shown.
    Menu { focus: usize },
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
    pub focus: usize,
    pub mode: Mode,
    /// The momentary button the click holds on: (component, control).
    pub pressed: Option<(String, String)>,
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

impl App {
    pub fn new(found: Vec<Discovered>, prefix: &str) -> App {
        let screens = found
            .into_iter()
            .filter_map(|d| {
                let items: Vec<Item> = d
                    .controls
                    .into_iter()
                    .filter_map(|control| {
                        let (widget, _) = pick_for(&d.component, &control);
                        (widget != Widget::Hidden).then(|| Item { label: control.name.clone(), control, widget })
                    })
                    .collect();
                let title = title(&d.component.name, prefix);
                (!items.is_empty()).then_some(Screen { component: d.component, title, items })
            })
            .collect();
        App { screens, screen: 0, focus: 0, mode: Mode::Browse, pressed: None }
    }

    pub fn current(&self) -> Option<&Screen> {
        self.screens.get(self.screen)
    }

    pub fn focused(&self) -> Option<&Item> {
        self.current()?.items.get(self.focus)
    }

    /// The controls to follow: every shown control, by component.
    pub fn watched(&self) -> Vec<(String, Vec<String>)> {
        self.screens
            .iter()
            .map(|s| (s.component.name.clone(), s.items.iter().map(|i| i.control.name.clone()).collect()))
            .collect()
    }

    pub fn handle(&mut self, ev: Event) -> Vec<Set> {
        if self.screens.is_empty() {
            return Vec::new();
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
                match ev {
                    Event::Turn(n) => self.mode = Mode::Menu { focus: wrap(focus, n, self.screens.len()) },
                    Event::Click => {
                        self.screen = focus;
                        self.focus = 0;
                        self.mode = Mode::Browse;
                    }
                    Event::Hold => self.mode = Mode::Browse,
                    _ => {}
                }
                Vec::new()
            }
            Mode::Edit { choice } => self.edit(ev, choice),
            Mode::Browse => self.browse(ev),
        }
    }

    fn browse(&mut self, ev: Event) -> Vec<Set> {
        let component = self.screens[self.screen].component.name.clone();
        let count = self.screens[self.screen].items.len();
        let Some(item) = self.screens[self.screen].items.get_mut(self.focus) else { return Vec::new() };
        let set = |item: &Item, value: Value| {
            vec![Set { component: component.clone(), control: item.control.name.clone(), value }]
        };
        match (ev, &item.widget) {
            (Event::Turn(n), _) => {
                self.focus = wrap(self.focus, n, count);
                Vec::new()
            }
            (Event::Hold, _) => {
                self.mode = Mode::Menu { focus: self.screen };
                Vec::new()
            }
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
            (Event::Click, Widget::Button) => set(item, json!(1)),
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
                let s = step(&item.control);
                let now = item.control.value.as_f64().unwrap_or(0.0);
                let mut v = ((now + n as f64 * s) / s).round() * s;
                if let Some((lo, hi)) = item.control.range() {
                    v = v.clamp(lo, hi);
                }
                // Clean off float noise from the rounding (0.30000000000000004).
                let v = (v * 1e6).round() / 1e6;
                item.control.value = json!(v);
                item.control.string = None;
                vec![Set { component, control: item.control.name.clone(), value: json!(v) }]
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
            for item in screen.items.iter_mut().filter(|i| i.control.name == ch.name) {
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
