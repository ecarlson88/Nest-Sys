//! What Designer 2026's emulation returned for a test design
//! (tests/data/designer), read as the screen reads it.

use qsys::{pick, Component, Control, Direction, Kind, Widget, PREFIX};
use serde_json::{json, Value};

fn controls(fixture: &str) -> Vec<Control> {
    let v: Value = serde_json::from_str(fixture).unwrap();
    v["Controls"].as_array().unwrap().iter().map(|c| Control::from_json(c).unwrap()).collect()
}

fn find<'a>(list: &'a [Control], name: &str) -> &'a Control {
    list.iter().find(|c| c.name == name).unwrap()
}

#[test]
fn reads_every_component_and_filters_by_prefix() {
    let v: Value = serde_json::from_str(include_str!("data/designer/components.json")).unwrap();
    let all: Vec<Component> = v.as_array().unwrap().iter().map(|c| Component::from_json(c).unwrap()).collect();
    assert_eq!(all.len(), 4);
    let kept: Vec<&str> = all.iter().filter(|c| c.name.starts_with(PREFIX)).map(|c| c.name.as_str()).collect();
    assert_eq!(kept, ["NEST_Panel", "NEST_Zone1"]);
}

#[test]
fn reads_a_gain_component() {
    let list = controls(include_str!("data/designer/controls_NEST_Zone1.json"));
    let gain = find(&list, "gain");
    assert_eq!(gain.kind, Kind::Number);
    assert_eq!(gain.range(), Some((-100.0, 20.0)));
    assert_eq!(gain.string_min.as_deref(), Some("-100dB"));
    assert_eq!(gain.string_max.as_deref(), Some("20.0dB"));
    assert_eq!(pick(gain).0, Widget::Dial { min: -100.0, max: 20.0 });
    let mute = find(&list, "mute");
    assert_eq!((mute.kind.clone(), mute.value.clone()), (Kind::Boolean, json!(false)));
    assert_eq!(mute.display(), "unmuted");
    assert_eq!(pick(mute), (Widget::Toggle, vec![]));
}

#[test]
fn reads_custom_controls() {
    let list = controls(include_str!("data/designer/controls_NEST_Panel.json"));
    let text = find(&list, "text.1");
    assert_eq!(text.kind, Kind::Text);
    assert_eq!(text.value, json!(""));
    let trigger = find(&list, "trigger.1");
    assert_eq!(trigger.kind, Kind::Trigger);
    assert_eq!(pick(trigger).0, Widget::Button);
    // Indicators report Read/Write, like inputs.
    for name in ["led.1", "meter.1", "text.display.1"] {
        assert_eq!(find(&list, name).direction, Direction::ReadWrite, "{name}");
    }
}
