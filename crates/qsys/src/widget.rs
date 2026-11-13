//! The widget the round screen shows for a control, from its kind, range and
//! direction (tools/qsys/qrc_discover.py's pick_widget does the same).
//!
//! | Control                      | Widget                                  |
//! |------------------------------|-----------------------------------------|
//! | Boolean, writable            | toggle (click flips)                    |
//! | Trigger, writable            | button (click fires)                    |
//! | Number with a range, writable| dial (ring adjusts, click confirms)     |
//! | Number without a range       | number entry (ring steps the value)     |
//! | Text, writable               | text entry (a picker of presets)        |
//! | Read-only                    | indicator, meter or label; none for a trigger |
//! | Status                       | label                                   |

use crate::control::{Control, Kind};

#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Toggle,
    Button,
    Dial { min: f64, max: f64 },
    NumberEntry,
    TextEntry,
    Indicator,
    Meter { min: f64, max: f64 },
    Label,
    /// Nothing to show: a read-only trigger.
    Hidden,
}

impl Widget {
    pub fn name(&self) -> &'static str {
        match self {
            Widget::Toggle => "toggle",
            Widget::Button => "button",
            Widget::Dial { .. } => "dial",
            Widget::NumberEntry => "number-entry",
            Widget::TextEntry => "text-entry",
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
        Kind::Text if writable => Widget::TextEntry,
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
