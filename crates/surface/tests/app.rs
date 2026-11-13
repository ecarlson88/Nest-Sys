//! The app on the design Designer's emulation described
//! (crates/qsys/tests/data/designer), driven as the ring and click would.

use qsys::{Change, Component, Control, Discovered, Widget, PREFIX};
use serde_json::{json, Value};
use surface::app::{App, Layout, Mode, Set};
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
    let now = app.last_input_ms;
    events.iter().flat_map(|e| app.handle(*e, now)).collect()
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
    // A gain is a volume knob: its gain and mute, its bypass followed.
    assert!(matches!(app.screens[1].layout, Layout::Volume { .. }));
    assert_eq!(app.watched()[1], ("NEST_Zone1".into(), vec!["gain".into(), "mute".into(), "bypass".into()]));
}

#[test]
fn the_ring_moves_between_controls_and_wraps() {
    let mut app = design();
    assert_eq!(focused(&app), "led.1");
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "meter.1");
    run(&mut app, &[Turn(-2)]);
    assert!(app.focused().is_none() && app.on_components_stop(), "after the last control");
    run(&mut app, &[Turn(-1)]);
    assert_eq!(focused(&app), "trigger.1");
    run(&mut app, &[Turn(2)]);
    assert_eq!(focused(&app), "led.1", "wraps past the Components stop");
}

#[test]
fn the_components_stop_opens_the_list() {
    let mut app = design();
    run(&mut app, &[Turn(-1)]);
    assert!(app.on_components_stop());
    assert!(run(&mut app, &[Down, Up, Click]).is_empty());
    assert_eq!(app.mode, Mode::Menu { focus: 0 });
}

#[test]
fn a_screen_of_only_a_momentary_can_still_be_left() {
    let momentary = Control::from_json(&json!({"Name": "momentary.1", "Type": "Boolean", "Value": false,
                                               "Direction": "Read/Write"})).unwrap();
    let found = vec![Discovered {
        component: Component { name: "NEST_Talk".into(), kind: Some("custom_controls".into()) },
        controls: vec![momentary],
    }];
    let mut app = App::new(found, PREFIX);
    assert!(run(&mut app, &[Down, Hold]).len() == 1, "pressed, and the hold keeps it pressed");
    assert_eq!(app.mode, Mode::Browse);
    run(&mut app, &[Up, Turn(1)]);
    assert!(app.on_components_stop());
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Menu { focus: 0 });
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
    app.handle(Click, 1_000);
    assert!(app.flash.is_some(), "a fired trigger flashes");
    assert!(!app.tick(1_100));
    assert!(app.tick(1_000 + surface::app::FLASH_MS));
    assert!(app.flash.is_none());
    assert_eq!(app.handle(Click, 2_000), [set("NEST_Panel", "trigger.1", json!(1))]);
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
fn a_gain_turns_with_the_ring_and_mutes_with_a_click() {
    let mut app = design();
    open(&mut app, "Zone1");
    // 0 dB in the fixture, a 120 dB range: steps of 1 dB, sent at once.
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "gain", json!(-1.0))]);
    assert_eq!(run(&mut app, &[Turn(3)]), [set("NEST_Zone1", "gain", json!(2.0))]);
    assert_eq!(run(&mut app, &[Turn(500)]), [set("NEST_Zone1", "gain", json!(20.0))]);
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("NEST_Zone1", "mute", json!(true))]);
    assert!(app.current().unwrap().muted());
    assert_eq!(run(&mut app, &[Down, Up, Click]), [set("NEST_Zone1", "mute", json!(false))]);
    // Muted, a turn unmutes and leaves the level; the next turn moves it.
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "mute", json!(false))]);
    assert!(!app.current().unwrap().muted());
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "gain", json!(19.0))]);
    assert_eq!(app.mode, Mode::Browse, "no editing on a volume screen");
    run(&mut app, &[Down, Hold, Up]);
    assert_eq!(app.mode, Mode::Menu { focus: 1 });
}

fn knobs() -> App {
    let knob = Control::from_json(&json!({"Name": "knob.1", "Type": "Float", "Value": 5.0, "ValueMin": 0.0,
                                          "ValueMax": 10.0, "Direction": "Read/Write"})).unwrap();
    let led = Control::from_json(&json!({"Name": "led.1", "Type": "Boolean", "Value": false,
                                         "Direction": "Read/Write"})).unwrap();
    App::new(vec![Discovered {
        component: Component { name: "NEST_Knobs".into(), kind: Some("custom_controls".into()) },
        controls: vec![knob, led],
    }], PREFIX)
}

#[test]
fn a_dial_steps_while_editing_and_stays_in_range() {
    let mut app = knobs();
    assert_eq!(focused(&app), "knob.1");
    assert!(run(&mut app, &[Turn(1), Turn(-1)]).is_empty(), "not editing yet");
    run(&mut app, &[Down, Up, Click]);
    assert!(matches!(app.mode, Mode::Edit { .. }));
    // A range of 10: steps of 0.1.
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Knobs", "knob.1", json!(4.9))]);
    assert_eq!(run(&mut app, &[Turn(500)]), [set("NEST_Knobs", "knob.1", json!(10.0))]);
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Browse);
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "led.1", "the ring moves on again");
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
    assert_eq!(app.mode, Mode::Menu { focus: 2 }, "a hold does nothing in the list");
    run(&mut app, &[Down, Up, Click]);
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.current().unwrap().title, "Combo");
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
        // Every control and the Components stop.
        for focus in 0..=app.screens[screen].items.len() {
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
fn a_gain_snaps_an_odd_value_onto_its_steps() {
    let mut app = design();
    open(&mut app, "Zone1");
    let gain = json!({"Component": "NEST_Zone1", "Name": "gain", "String": "-59.4dB", "Value": -59.36000061});
    app.apply(&Change::from_json(&gain).unwrap());
    assert_eq!(run(&mut app, &[Turn(-1)]), [set("NEST_Zone1", "gain", json!(-60.0))]);
}

#[test]
fn editing_ends_after_five_seconds_without_input() {
    let mut app = knobs();
    app.handle(Click, 1_000);
    assert!(matches!(app.mode, Mode::Edit { .. }));
    assert!(!app.tick(5_999), "turns and clicks keep it open");
    app.handle(Turn(1), 5_000);
    assert!(!app.tick(9_999));
    assert!(app.tick(10_000));
    assert_eq!(app.mode, Mode::Browse);
    // Turning now moves to the next control instead of changing the knob.
    assert!(app.handle(Turn(1), 10_500).is_empty());
    assert_eq!(focused(&app), "led.1");
    assert!(!app.tick(60_000), "browsing does not time out");
}

#[test]
fn a_timed_out_picker_sends_nothing() {
    let mut app = design();
    open(&mut app, "Combo");
    app.handle(Click, 0);
    app.handle(Turn(1), 100);
    assert!(app.tick(5_100));
    assert_eq!(app.focused().unwrap().control.string.as_deref(), Some("Streaming"));
}

fn change(v: Value) -> Change {
    Change::from_json(&v).unwrap()
}

#[test]
fn invisible_and_disabled_controls_are_skipped() {
    let mut app = design();
    let hide = |name: &str, flag: &str, on: bool| {
        change(json!({"Component": "NEST_Panel", "Name": name, "String": "false", "Value": 0.0, flag: on}))
    };
    app.apply(&hide("meter.1", "Invisible", true));
    app.apply(&hide("momentary.1", "Disabled", true));
    run(&mut app, &[Turn(1)]);
    assert_eq!(focused(&app), "text.1", "meter and momentary skipped");
    // The focused control hidden: the focus moves on to the next.
    app.apply(&hide("text.1", "Invisible", true));
    assert_eq!(focused(&app), "text.display.1");
    app.apply(&hide("meter.1", "Invisible", false));
    run(&mut app, &[Turn(-1)]);
    assert_eq!(focused(&app), "meter.1", "shown again");
}

#[test]
fn a_bypassed_component_is_left_out() {
    let mut app = design();
    open(&mut app, "Zone1");
    let bypass = |on: f64| change(json!({"Component": "NEST_Zone1", "Name": "bypass", "String": "", "Value": on}));
    app.apply(&bypass(1.0));
    assert_eq!(app.current().unwrap().title, "Panel", "moved off the bypassed screen");
    assert_eq!(app.shown_screens(), [0, 2]);
    run(&mut app, &[Down, Hold, Up, Turn(1)]);
    assert_eq!(app.mode, Mode::Menu { focus: 2 }, "the list skips it");
    app.apply(&bypass(0.0));
    assert_eq!(app.shown_screens(), [0, 1, 2]);
}

#[test]
fn nothing_shown_when_everything_is_hidden() {
    let mut app = knobs();
    for name in ["knob.1", "led.1"] {
        app.apply(&change(json!({"Component": "NEST_Knobs", "Name": name, "String": "", "Value": 0.0, "Invisible": true})));
    }
    assert!(app.current().is_none());
    assert!(run(&mut app, &[Turn(1), Down, Up, Click]).is_empty());
    render(&app, &mut Canvas::new(FACE));
}
