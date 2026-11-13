//! Named Components and their controls as Component.GetComponents and
//! Component.GetControls describe them.
//!
//! Fields as Designer 2026's emulation returns them (tests/data/designer):
//! every control has Name, Type, Direction and String; Float and Integer add
//! ValueMin/ValueMax and StringMin/StringMax; Text has no Value, only String;
//! a Trigger's Value is 0.0 and it has no String. Direction is "Read/Write",
//! "Read Only" or "Write Only", and Custom Controls' indicators (LED, meter,
//! display text) are Read/Write too. Script components have controls without
//! a Type. Every field but Name stays optional; missing ones are reported
//! (see widget::Note) rather than refused.
//!
//! GetControls never gives a combo box's Choices; Component.Get and change
//! group polls do (see client::Change), and Transport::list fills them in.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// The component's Code Name.
    pub name: String,
    /// Its kind as the Core names it (e.g. "gain"), when given.
    pub kind: Option<String>,
}

impl Component {
    pub fn from_json(v: &Value) -> Option<Component> {
        Some(Component {
            name: v.get("Name")?.as_str()?.to_owned(),
            kind: v.get("Type").and_then(Value::as_str).map(str::to_owned),
        })
    }
}

/// What a control holds, from its Type, or from its Value's JSON type when
/// Type is missing or unfamiliar.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Boolean,
    /// Momentary: setting it fires once.
    Trigger,
    Text,
    Number,
    /// A component's health ("OK", "Fault", ...): read only.
    Status,
    Unknown,
}

impl Kind {
    fn of(type_name: Option<&str>, value: Option<&Value>) -> Kind {
        let t = type_name.unwrap_or("").trim().to_ascii_lowercase();
        match t.as_str() {
            "boolean" => return Kind::Boolean,
            "trigger" => return Kind::Trigger,
            "text" => return Kind::Text,
            "float" | "integer" => return Kind::Number,
            "status" => return Kind::Status,
            _ => {}
        }
        match value {
            Some(Value::Bool(_)) => Kind::Boolean,
            Some(Value::Number(_)) => Kind::Number,
            Some(Value::String(_)) => Kind::Text,
            _ => Kind::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Direction {
    ReadWrite,
    ReadOnly,
    WriteOnly,
    /// No Direction field: treated as writable.
    Unknown,
}

impl Direction {
    fn parse(s: Option<&str>) -> Direction {
        let Some(s) = s else { return Direction::Unknown };
        let s = s.to_ascii_lowercase();
        match (s.contains("read"), s.contains("write")) {
            (true, true) => Direction::ReadWrite,
            (true, false) => Direction::ReadOnly,
            (false, true) => Direction::WriteOnly,
            (false, false) => Direction::Unknown,
        }
    }

    pub fn writable(&self) -> bool {
        *self != Direction::ReadOnly
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub name: String,
    /// The Type field as given, for display.
    pub type_name: Option<String>,
    pub kind: Kind,
    /// Value, or for a control without one (Text), its String.
    pub value: Value,
    /// The Core's own rendering of the value (e.g. "-12.0dB").
    pub string: Option<String>,
    /// 0..1 along the control's range.
    pub position: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// A combo box's options: from Component.Get (GetControls has none).
    pub choices: Option<Vec<String>>,
    /// The ends of the range as the Core renders them (e.g. "-100dB").
    pub string_min: Option<String>,
    pub string_max: Option<String>,
    pub direction: Direction,
    pub has_direction: bool,
}

impl Control {
    pub fn from_json(v: &Value) -> Option<Control> {
        let type_name = v.get("Type").and_then(Value::as_str);
        let direction = v.get("Direction").and_then(Value::as_str);
        let string = v.get("String").and_then(Value::as_str).map(str::to_owned);
        let value = match (v.get("Value"), &string) {
            (Some(value), _) => value.clone(),
            (None, Some(s)) => Value::String(s.clone()),
            (None, None) => Value::Null,
        };
        let text = |key| v.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Control {
            name: v.get("Name")?.as_str()?.to_owned(),
            type_name: type_name.map(str::to_owned),
            kind: Kind::of(type_name, Some(&value)),
            value,
            string,
            position: v.get("Position").and_then(Value::as_f64),
            min: v.get("ValueMin").and_then(Value::as_f64),
            max: v.get("ValueMax").and_then(Value::as_f64),
            choices: v.get("Choices").and_then(Value::as_array).map(|a| {
                a.iter().filter_map(Value::as_str).map(str::to_owned).collect()
            }),
            string_min: text("StringMin"),
            string_max: text("StringMax"),
            direction: Direction::parse(direction),
            has_direction: direction.is_some(),
        })
    }

    /// The value as on or off: Booleans are true/false from GetControls but
    /// 0.0/1.0 from Component.Get and polls.
    pub fn is_on(&self) -> bool {
        truthy(&self.value)
    }

    /// The range when both ends are given and min < max.
    pub fn range(&self) -> Option<(f64, f64)> {
        match (self.min, self.max) {
            (Some(lo), Some(hi)) if hi > lo => Some((lo, hi)),
            _ => None,
        }
    }

    /// The value as shown: the Core's String, else the raw Value.
    pub fn display(&self) -> String {
        match (&self.string, &self.value) {
            (Some(s), _) => s.clone(),
            (None, Value::Null) => String::new(),
            (None, Value::String(s)) => s.clone(),
            (None, v) => v.to_string(),
        }
    }
}

/// A Boolean control's value in any of the forms the Core sends.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_every_documented_field() {
        let c = Control::from_json(&json!({
            "Name": "gain", "Type": "Float", "Value": -12.0, "ValueMin": -100.0,
            "ValueMax": 20.0, "String": "-12.0dB", "Position": 0.77, "Direction": "Read/Write"
        }))
        .unwrap();
        assert_eq!(c.kind, Kind::Number);
        assert_eq!(c.range(), Some((-100.0, 20.0)));
        assert_eq!(c.direction, Direction::ReadWrite);
        assert_eq!(c.display(), "-12.0dB");
    }

    #[test]
    fn takes_a_text_controls_value_from_its_string() {
        let c = Control::from_json(&json!({"Name": "text.1", "Type": "Text", "String": "Hello",
                                           "Direction": "Read/Write"}))
        .unwrap();
        assert_eq!(c.value, json!("Hello"));
        assert_eq!(c.kind, Kind::Text);
    }

    #[test]
    fn falls_back_to_the_value_without_a_type() {
        let kind = |v| Control::from_json(&json!({"Name": "x", "Value": v})).unwrap().kind;
        assert_eq!(kind(json!(true)), Kind::Boolean);
        assert_eq!(kind(json!(3)), Kind::Number);
        assert_eq!(kind(json!("hi")), Kind::Text);
        assert_eq!(kind(json!(null)), Kind::Unknown);
    }

    #[test]
    fn reads_direction_spellings() {
        let dir = |d: &str| Control::from_json(&json!({"Name": "x", "Direction": d})).unwrap().direction;
        assert_eq!(dir("Read Only"), Direction::ReadOnly);
        assert_eq!(dir("Read/Write"), Direction::ReadWrite);
        assert_eq!(dir("Write Only"), Direction::WriteOnly);
    }

    #[test]
    fn needs_a_name() {
        assert!(Control::from_json(&json!({"Value": 1})).is_none());
        assert!(Component::from_json(&json!({"Type": "gain"})).is_none());
    }
}
