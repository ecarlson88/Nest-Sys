//! The widget the round screen shows for a control, from its kind, range and
//! direction (tools/qsys/qrc_discover.py's pick_widget does the same).
//!
//! Type and Direction cannot tell an indicator from an input: Custom
//! Controls' LEDs, meters and display text are Read/Write, and its toggles
//! and momentary buttons are both Boolean. [`pick_for`] reads those from the
//! names Q-SYS gives Custom Controls (`led.1`, `meter.1`, `momentary.1`,
//! `text.display.1`) and hides a script controller's own controls (code,
//! log, script status); [`pick`] uses the control alone.
//!
//! | Control                      | Widget                                  |
//! |------------------------------|-----------------------------------------|
//! | Boolean, writable            | toggle (click flips)                    |
//! | Trigger, writable            | button (click fires)                    |
//! | Number with a range, writable| dial (ring adjusts, click confirms)     |
//! | Number without a range       | number entry (ring steps the value)     |
//! | Text with Choices, writable  | picker (ring scrolls, click selects)    |
//! | Text, writable               | text entry (a picker of presets)        |
//! | Read-only                    | indicator, meter or label; none for a trigger |
//! | Status                       | label                                   |

use crate::control::{Component, Control, Kind};

#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Toggle,
    Button,
    /// Pressed while the click is held: true on press, false on release.
    Momentary,
    Dial { min: f64, max: f64 },
    NumberEntry,
    TextEntry,
    Picker { choices: Vec<String> },
    Indicator,
    Meter { min: f64, max: f64 },
    Label,
    /// Nothing to show: a read-only trigger, a script's own controls.
    Hidden,
}

impl Widget {
    pub fn name(&self) -> &'static str {
        match self {
            Widget::Toggle => "toggle",
            Widget::Button => "button",
            Widget::Momentary => "momentary",
            Widget::Dial { .. } => "dial",
            Widget::NumberEntry => "number-entry",
            Widget::TextEntry => "text-entry",
            Widget::Picker { .. } => "picker",
            Widget::Indicator => "indicator",
            Widget::Meter { .. } => "meter",
            Widget::Label => "label",
            Widget::Hidden => "-",
        }
    }
}

/// Metadata the choice wanted but the Core did not give.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    NoType,
    NoDirection,
    NoRange,
    Unrecognised,
}

impl Note {
    pub fn text(&self) -> &'static str {
        match self {
            Note::NoType => "no Type field",
            Note::NoDirection => "no Direction field",
            Note::NoRange => "no ValueMin/ValueMax",
            Note::Unrecognised => "unrecognised",
        }
    }
}

pub fn pick(c: &Control) -> (Widget, Vec<Note>) {
    let mut notes = Vec::new();
    if c.type_name.as_deref().is_none_or(|t| t.trim().is_empty()) {
        notes.push(Note::NoType);
    }
    if !c.has_direction {
        notes.push(Note::NoDirection);
    }
    let writable = c.direction.writable();
    let widget = match c.kind {
        Kind::Boolean if writable => Widget::Toggle,
        Kind::Boolean => Widget::Indicator,
        Kind::Trigger if writable => Widget::Button,
        Kind::Trigger => Widget::Hidden,
        Kind::Text if writable => match &c.choices {
            Some(choices) if !choices.is_empty() => Widget::Picker { choices: choices.clone() },
            _ => Widget::TextEntry,
        },
        Kind::Text => Widget::Label,
        Kind::Status => Widget::Label,
        Kind::Number => match (c.range(), writable) {
            (Some((min, max)), true) => Widget::Dial { min, max },
            (Some((min, max)), false) => Widget::Meter { min, max },
            (None, true) => {
                notes.push(Note::NoRange);
                Widget::NumberEntry
            }
            (None, false) => Widget::Label,
        },
        Kind::Unknown => {
            notes.push(Note::Unrecognised);
            Widget::Label
        }
    };
    (widget, notes)
}

/// The component type Q-SYS gives Custom Controls.
pub const CUSTOM_CONTROLS: &str = "custom_controls";
/// The component type of a Text Controller (and other script components).
pub const SCRIPT: &str = "device_controller_script";

/// A control's name without its index: "text.display" for "text.display.1".
fn base_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((base, index)) if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => name,
    }
}

/// A script component's own controls, there whatever the script defines.
fn script_internal(name: &str) -> bool {
    matches!(name, "code" | "reload" | "halt.catch.fire" | "magic.number")
        || name.starts_with("script.")
        || name.starts_with("log.")
}

/// pick, knowing the control's component: see the module's notes.
pub fn pick_for(component: &Component, c: &Control) -> (Widget, Vec<Note>) {
    let (widget, notes) = pick(c);
    match component.kind.as_deref() {
        Some(CUSTOM_CONTROLS) => {
            let widget = match (base_name(&c.name), widget) {
                ("led", _) => Widget::Indicator,
                ("meter", w) => c.range().map_or(w, |(min, max)| Widget::Meter { min, max }),
                ("text.display", _) => Widget::Label,
                ("momentary", Widget::Toggle) => Widget::Momentary,
                (_, w) => w,
            };
            (widget, notes)
        }
        Some(SCRIPT) if script_internal(&c.name) => (Widget::Hidden, Vec::new()),
        _ => (widget, notes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn widget(v: Value) -> (Widget, Vec<Note>) {
        pick(&Control::from_json(&v).unwrap())
    }

    #[test]
    fn follows_the_v1_mapping() {
        let rw = "Read/Write";
        let ro = "Read Only";
        assert_eq!(widget(json!({"Name": "m", "Type": "Boolean", "Direction": rw})).0, Widget::Toggle);
        assert_eq!(widget(json!({"Name": "m", "Type": "Boolean", "Direction": ro})).0, Widget::Indicator);
        assert_eq!(widget(json!({"Name": "t", "Type": "Trigger", "Direction": rw})).0, Widget::Button);
        assert_eq!(widget(json!({"Name": "t", "Type": "Trigger", "Direction": ro})).0, Widget::Hidden);
        assert_eq!(widget(json!({"Name": "s", "Type": "Text", "Direction": rw})).0, Widget::TextEntry);
        assert_eq!(widget(json!({"Name": "s", "Type": "Text", "Direction": ro})).0, Widget::Label);
        assert_eq!(
            widget(json!({"Name": "g", "Type": "Float", "ValueMin": -100, "ValueMax": 20, "Direction": rw})).0,
            Widget::Dial { min: -100.0, max: 20.0 }
        );
        assert_eq!(
            widget(json!({"Name": "l", "Type": "Float", "ValueMin": -100, "ValueMax": 20, "Direction": ro})).0,
            Widget::Meter { min: -100.0, max: 20.0 }
        );
        assert_eq!(widget(json!({"Name": "n", "Type": "Integer", "Direction": ro})).0, Widget::Label);
        assert_eq!(
            widget(json!({"Name": "script.status", "Type": "Status", "ValueMin": 0, "ValueMax": 5,
                          "String": "OK", "Direction": ro})).0,
            Widget::Label
        );
    }

    #[test]
    fn offers_choices_as_a_picker() {
        let (w, _) = widget(json!({"Name": "Control.1", "Type": "Text", "Choices": ["A", "B"],
                                   "Direction": "Read/Write"}));
        assert_eq!(w, Widget::Picker { choices: vec!["A".into(), "B".into()] });
    }

    fn in_component(kind: &str, v: Value) -> Widget {
        let comp = Component { name: "NEST_X".into(), kind: Some(kind.into()) };
        pick_for(&comp, &Control::from_json(&v).unwrap()).0
    }

    #[test]
    fn reads_custom_controls_by_their_names() {
        let rw = "Read/Write";
        let cc = CUSTOM_CONTROLS;
        assert_eq!(in_component(cc, json!({"Name": "led.3", "Type": "Boolean", "Direction": rw})), Widget::Indicator);
        assert_eq!(
            in_component(cc, json!({"Name": "meter.1", "Type": "Float", "ValueMin": -120, "ValueMax": 20, "Direction": rw})),
            Widget::Meter { min: -120.0, max: 20.0 }
        );
        assert_eq!(in_component(cc, json!({"Name": "text.display.1", "Type": "Text", "Direction": rw})), Widget::Label);
        assert_eq!(in_component(cc, json!({"Name": "momentary.12", "Type": "Boolean", "Direction": rw})), Widget::Momentary);
        assert_eq!(in_component(cc, json!({"Name": "toggle.1", "Type": "Boolean", "Direction": rw})), Widget::Toggle);
        assert_eq!(in_component(cc, json!({"Name": "text.1", "Type": "Text", "Direction": rw})), Widget::TextEntry);
        // The names mean nothing in other components.
        assert_eq!(in_component("gain", json!({"Name": "led.1", "Type": "Boolean", "Direction": rw})), Widget::Toggle);
    }

    #[test]
    fn hides_a_script_components_own_controls() {
        let rw = "Read/Write";
        for name in ["code", "reload", "log.history", "script.status", "script.blockly.code", "magic.number"] {
            assert_eq!(in_component(SCRIPT, json!({"Name": name, "Type": "Text", "Direction": rw})), Widget::Hidden, "{name}");
        }
        assert_eq!(in_component(SCRIPT, json!({"Name": "Control.1", "Type": "Text", "Direction": rw})), Widget::TextEntry);
    }

    #[test]
    fn strips_only_a_numeric_index() {
        assert_eq!(base_name("text.display.1"), "text.display");
        assert_eq!(base_name("led.12"), "led");
        assert_eq!(base_name("gain"), "gain");
        assert_eq!(base_name("text.display"), "text.display");
    }

    #[test]
    fn notes_a_number_without_a_range() {
        let (w, notes) = widget(json!({"Name": "n", "Type": "Integer", "Direction": "Read/Write"}));
        assert_eq!(w, Widget::NumberEntry);
        assert_eq!(notes, vec![Note::NoRange]);
    }

    #[test]
    fn an_empty_range_is_no_range() {
        let (w, _) = widget(json!({"Name": "n", "Type": "Float", "ValueMin": 5, "ValueMax": 5, "Direction": "Read/Write"}));
        assert_eq!(w, Widget::NumberEntry);
    }

    #[test]
    fn notes_missing_metadata_and_guesses_from_the_value() {
        let (w, notes) = widget(json!({"Name": "x", "Value": true}));
        assert_eq!(w, Widget::Toggle);
        assert_eq!(notes, vec![Note::NoType, Note::NoDirection]);
        let (w, notes) = widget(json!({"Name": "x", "Type": "Mystery", "Direction": "Read/Write"}));
        assert_eq!(w, Widget::Label);
        assert_eq!(notes, vec![Note::Unrecognised]);
    }
}
