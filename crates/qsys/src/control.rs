//! Named Components and their controls as Component.GetComponents and
//! Component.GetControls describe them.
//!
//! The QRC docs show Name, Value, String, Position and Direction for a
//! control; Type, ValueMin and ValueMax are expected but not yet seen from a
//! real Core, so every field but Name is optional and missing ones are
//! reported (see widget::Note) rather than refused.

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
    Unknown,
}

impl Kind {
    fn of(type_name: Option<&str>, value: Option<&Value>) -> Kind {
        let t = type_name.unwrap_or("").trim().to_ascii_lowercase();
        match t.as_str() {
            "boolean" | "bool" | "toggle" => return Kind::Boolean,
            "trigger" | "momentary" => return Kind::Trigger,
            "text" | "string" => return Kind::Text,
            "float" | "integer" | "int" | "number" | "knob" | "state" => return Kind::Number,
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
    pub value: Value,
    /// The Core's own rendering of the value (e.g. "-12.0dB").
    pub string: Option<String>,
    /// 0..1 along the control's range.
    pub position: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub direction: Direction,
    pub has_direction: bool,
}

impl Control {
    pub fn from_json(v: &Value) -> Option<Control> {
        let type_name = v.get("Type").and_then(Value::as_str);
        let direction = v.get("Direction").and_then(Value::as_str);
        Some(Control {
            name: v.get("Name")?.as_str()?.to_owned(),
            type_name: type_name.map(str::to_owned),
            kind: Kind::of(type_name, v.get("Value")),
            value: v.get("Value").cloned().unwrap_or(Value::Null),
            string: v.get("String").and_then(Value::as_str).map(str::to_owned),
            position: v.get("Position").and_then(Value::as_f64),
            min: v.get("ValueMin").and_then(Value::as_f64),
            max: v.get("ValueMax").and_then(Value::as_f64),
            direction: Direction::parse(direction),
            has_direction: direction.is_some(),
        })
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
