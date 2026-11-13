//! The app on the design Designer's emulation described
//! (crates/qsys/tests/data/designer), driven as the ring and click would.

use qsys::{Change, Component, Control, Discovered, Widget, PREFIX};
use serde_json::{json, Value};
use surface::app::{App, Mode, Set};
use surface::canvas::{Canvas, FACE};
use surface::input::Event::{self, *};
use surface::render::render;

fn load(fixture: &str) -> Value {
    serde_json::from_str(fixture).unwrap()
}

fn design() -> App {
    let comps = load(include_str!("../../qsys/tests/data/designer/components.json"));
    let kind = |name: &str| {
        comps.as_array().unwrap().iter().find(|c| c["Name"] == name).unwrap()["Type"].as_str().map(str::to_owned)
    };
    let controls = |fixture: &str| -> Vec<Control> {
        load(fixture)["Controls"].as_array().unwrap().iter().map(|c| Control::from_json(c).unwrap()).collect()
    };
    let mut combo = controls(include_str!("../../qsys/tests/data/designer/controls_Nest_Combo.json"));
    // As Transport::list fills it in from Component.Get.
    let get = load(include_str!("../../qsys/tests/data/designer/get_Nest_Combo_Control.json"));
    let choices: Vec<String> =
        serde_json::from_value(get["Controls"][0]["Choices"].clone()).unwrap();
    combo.iter_mut().find(|c| c.name == "Control").unwrap().choices = Some(choices);
    let found = vec![
        Discovered {
            component: Component { name: "NEST_Panel".into(), kind: kind("NEST_Panel") },
            controls: controls(include_str!("../../qsys/tests/data/designer/controls_NEST_Panel.json")),
        },
        Discovered {
            component: Component { name: "NEST_Zone1".into(), kind: kind("NEST_Zone1") },
            controls: controls(include_str!("../../qsys/tests/data/designer/controls_NEST_Zone1.json")),
        },
        Discovered { component: Component { name: "Nest_Combo".into(), kind: kind("Nest_Combo") }, controls: combo },
    ];
    App::new(found, PREFIX)
}

fn run(app: &mut App, events: &[Event]) -> Vec<Set> {
    events.iter().flat_map(|e| app.handle(*e)).collect()
}

fn set(component: &str, control: &str, value: Value) -> Set {
    Set { component: component.into(), control: control.into(), value }
}

fn focused(app: &App) -> &str {
    &app.focused().unwrap().control.name
}

/// Opens a screen through the list of components.
fn open(app: &mut App, title: &str) {
    run(app, &[Down, Hold, Up]);
    let Mode::Menu { focus } = app.mode else { panic!("no menu") };
    let target = app.screens.iter().position(|s| s.title == title).unwrap() as i32;
    run(app, &[Turn(target - focus as i32), Down, Up, Click]);
    assert_eq!(app.current().unwrap().title, title);
    assert_eq!(app.mode, Mode::Browse);
}

#[test]
fn has_a_screen_per_component_without_script_internals() {
    let app = design();
    let titles: Vec<&str> = app.screens.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Panel", "Zone1", "Combo"]);
    let combo: Vec<(&str, &Widget)> =
        app.screens[2].items.iter().map(|i| (i.control.name.as_str(), &i.widget)).collect();
    assert_eq!(combo.len(), 1);
    assert_eq!(combo[0].0, "Control");
    assert!(matches!(combo[0].1, Widget::Picker { .. }));
    assert_eq!(app.watched()[1], ("NEST_Zone1".into(), vec!["bypass".into(), "gain".into(), "invert".into(), "mute".into()]));
}

#[test]
fn the_ring_moves_between_controls_and_wraps() {
    let mut app = design();
    assert_eq!(focused(&app), "led.1");
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "meter.1");
    run(&mut app, &[Turn(-2)]);
    assert_eq!(focused(&app), "trigger.1");
}

#[test]
fn a_click_flips_a_toggle_and_fires_a_trigger() {
    let mut app = design();
    run(&mut app, &[Turn(5)]);
    assert_eq!(focused(&app), "toggle.1");
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("NEST_Panel", "toggle.1", json!(true))]);
    assert!(app.focused().unwrap().control.is_on());
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("NEST_Panel", "toggle.1", json!(false))]);
    run(&mut app, &[Turn(1)]);
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("NEST_Panel", "trigger.1", json!(1))]);
}

#[test]
fn indicators_do_nothing_when_clicked() {
    let mut app = design();
    for _ in 0..2 {
        assert!(run(&mut app, &[Down, Up, Click]).is_empty(), "{}", focused(&app));
        run(&mut app, &[Turn(1)]);
    }
}

#[test]
fn a_dial_steps_while_editing_and_stays_in_range() {
    let mut app = design();
    open(&mut app, "Zone1");
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "gain");
    // 0 dB in the fixture, a 120 dB range: steps of 1 dB.
    assert!(run(&mut app, &[Turn(1)]).is_empty(), "not editing yet");
    run(&mut app, &[Turn(-1), Down, Up, Click]);
    assert!(matches!(app.mode, Mode::Edit { .. }));
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "gain", json!(-1.0))]);
    assert_eq!(run(&mut app, &[Turn(3)]), [set("NEST_Zone1", "gain", json!(2.0))]);
    assert_eq!(run(&mut app, &[Turn(500)]), [set("NEST_Zone1", "gain", json!(20.0))]);
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Browse);
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "invert", "the ring moves on again");
}

#[test]
fn a_picker_sends_the_choice_clicked_and_a_hold_cancels() {
    let mut app = design();
    open(&mut app, "Combo");
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Edit { choice: 3 }, "on the current choice, Streaming");
    assert!(run(&mut app, &[Turn(3)]).is_empty(), "nothing sent while choosing");
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("Nest_Combo", "Control", json!("Laptop"))]);
    assert_eq!(app.focused().unwrap().control.string.as_deref(), Some("Laptop"));
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Edit { choice: 2 }, "opens on the current choice");
    assert!(run(&mut app, &[Turn(1), Down, Hold, Up]).is_empty());
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.focused().unwrap().control.string.as_deref(), Some("Laptop"));
}

#[test]
fn a_momentary_is_on_while_held_and_never_left_on() {
    let mut app = design();
    run(&mut app, &[Turn(2)]);
    assert_eq!(focused(&app), "momentary.1");
    assert_eq!(run(&mut app, &[Down]), [set("NEST_Panel", "momentary.1", json!(true))]);
    // Held long: still pressed, no menu.
    assert!(run(&mut app, &[Hold]).is_empty());
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(run(&mut app, &[Up]), [set("NEST_Panel", "momentary.1", json!(false))]);
    // Pressed, then the ring turned away: the release still turns it off.
    run(&mut app, &[Down, Turn(1)]);
    assert_eq!(run(&mut app, &[Up]), [set("NEST_Panel", "momentary.1", json!(false))]);
}

#[test]
fn the_list_of_components_opens_and_closes() {
    let mut app = design();
    run(&mut app, &[Down, Hold, Up]);
    assert_eq!(app.mode, Mode::Menu { focus: 0 });
    run(&mut app, &[Turn(-1)]);
    assert_eq!(app.mode, Mode::Menu { focus: 2 });
    run(&mut app, &[Down, Hold, Up]);
    assert_eq!(app.mode, Mode::Browse, "a hold goes back");
    assert_eq!(app.current().unwrap().title, "Panel");
}

#[test]
fn applies_the_cores_changes_as_designer_sends_them() {
    let mut app = design();
    let poll = load(include_str!("../../qsys/tests/data/designer/poll_combo.json"));
    app.apply(&Change::from_json(&poll[0]).unwrap());
    let combo = &app.screens[2].items[0].control;
    assert_eq!(combo.value, json!("Mic 2"), "text from String, not the 0.0 Value");

    let on = json!({"Component": "NEST_Zone1", "Name": "mute", "String": "muted", "Value": 1.0, "Legend": "Mute"});
    app.apply(&Change::from_json(&on).unwrap());
    let mute = app.screens[1].items.iter().find(|i| i.control.name == "mute").unwrap();
    assert!(mute.control.is_on());
    assert_eq!(mute.label, "Mute");

    let new_list = json!({"Component": "Nest_Combo", "Name": "Control", "String": "A", "Value": 0.0, "Choices": ["A", "B"]});
    app.apply(&Change::from_json(&new_list).unwrap());
    assert_eq!(app.screens[2].items[0].widget, Widget::Picker { choices: vec!["A".into(), "B".into()] });
}

#[test]
fn renders_every_control_in_every_mode() {
    let mut app = design();
    let mut canvas = Canvas::new(FACE);
    for screen in 0..app.screens.len() {
        app.screen = screen;
        for focus in 0..app.screens[screen].items.len() {
            app.focus = focus;
            for mode in [Mode::Browse, Mode::Edit { choice: 1 }, Mode::Menu { focus: screen }] {
                app.mode = mode;
                render(&app, &mut canvas);
                assert_eq!(canvas.get(0, 0), 0, "outside the circle stays black");
            }
        }
    }
    app.screens.clear();
    render(&app, &mut canvas);
}

#[test]
fn a_dial_snaps_an_odd_value_onto_its_steps() {
    let mut app = design();
    open(&mut app, "Zone1");
    run(&mut app, &[Turn(1), Down, Up, Click]);
    let gain = json!({"Component": "NEST_Zone1", "Name": "gain", "String": "-59.4dB", "Value": -59.36000061});
    app.apply(&Change::from_json(&gain).unwrap());
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "gain", json!(-60.0))]);
}
