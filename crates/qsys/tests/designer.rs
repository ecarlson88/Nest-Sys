//! What Designer 2026's emulation returned for a test design
//! (tests/data/designer), read as the screen reads it.

use qsys::{has_prefix, pick, pick_for, Change, Component, Control, Direction, Kind, Widget, PREFIX};
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
    let kept: Vec<&str> = all.iter().filter(|c| has_prefix(&c.name, PREFIX)).map(|c| c.name.as_str()).collect();
    assert_eq!(kept, ["NEST_Panel", "NEST_Zone1", "Nest_Combo"]);
    // NEST_Hidden, with Script Access None, is not listed at all.
    assert!(!all.iter().any(|c| c.name.eq_ignore_ascii_case("NEST_Hidden")));
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

#[test]
fn picks_custom_controls_widgets_by_name() {
    let panel = Component { name: "NEST_Panel".into(), kind: Some("custom_controls".into()) };
    let list = controls(include_str!("data/designer/controls_NEST_Panel.json"));
    let widgets: Vec<(&str, Widget)> = list.iter().map(|c| (c.name.as_str(), pick_for(&panel, c).0)).collect();
    assert_eq!(
        widgets,
        [
            ("led.1", Widget::Indicator),
            ("meter.1", Widget::Meter { min: -120.0, max: 20.0 }),
            ("momentary.1", Widget::Momentary),
            ("text.1", Widget::TextEntry),
            ("text.display.1", Widget::Label),
            ("toggle.1", Widget::Toggle),
            ("trigger.1", Widget::Button),
        ]
    );
}

#[test]
fn shows_only_a_text_controllers_own_controls() {
    let combo = Component { name: "Nest_Combo".into(), kind: Some("device_controller_script".into()) };
    let list = controls(include_str!("data/designer/controls_Nest_Combo.json"));
    assert_eq!(list.len(), 19);
    let shown: Vec<(&str, Widget)> = list
        .iter()
        .map(|c| (c.name.as_str(), pick_for(&combo, c).0))
        .filter(|(_, w)| *w != Widget::Hidden)
        .collect();
    // A combo box: Designer reports no Choices, so it is plain text entry.
    assert_eq!(shown, [("Control", Widget::TextEntry)]);
    assert_eq!(find(&list, "Control").value, json!("Streaming"));
}

fn changes(fixture: &str) -> Vec<Change> {
    let v: Value = serde_json::from_str(fixture).unwrap();
    v.as_array().unwrap().iter().map(|c| Change::from_json(c).unwrap()).collect()
}

#[test]
fn reads_polls_as_designer_sends_them() {
    let first = changes(include_str!("data/designer/poll_first.json"));
    assert_eq!(first.len(), 12);
    let get = |name: &str| first.iter().find(|c| c.name == name).unwrap();
    // Booleans arrive as numbers, text as String with a 0.0 Value.
    assert_eq!(get("mute").value, json!(0.0));
    assert!(!get("mute").is_on());
    assert_eq!(get("text.1").string.as_deref(), Some("sfhd9ufg"));
    assert_eq!(get("led.1").choices.as_deref(), Some(&[][..]));
    assert_eq!(get("gain").choices, None);
    assert_eq!(get("Control").choices.as_ref().unwrap().len(), 4);
    assert_eq!(get("Control").legend, None);

    let combo = changes(include_str!("data/designer/poll_combo.json"));
    assert_eq!(combo[0].component.as_deref(), Some("Nest_Combo"));
    assert_eq!(combo[0].string.as_deref(), Some("Mic 2"));
}

#[test]
fn gets_a_combo_boxs_choices() {
    let v: Value = serde_json::from_str(include_str!("data/designer/get_Nest_Combo_Control.json")).unwrap();
    let c = Change::from_json(&v["Controls"][0]).unwrap();
    assert_eq!(c.choices.unwrap(), ["Mic 1", "Mic 2", "Laptop", "Streaming"]);
    assert_eq!(c.string.as_deref(), Some("Mic 1"));
}
