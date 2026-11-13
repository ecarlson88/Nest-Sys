//! The client and transport against a fake Core.

mod mock;

use std::time::Duration;

use mock::MockCore;
use qsys::{pick, pick_for, Client, Error, Qrc, Transport, Widget, PREFIX};
use serde_json::json;

fn connect(core: &MockCore) -> Qrc<std::net::TcpStream> {
    Qrc::new(Client::connect("127.0.0.1", core.port, Duration::from_secs(5)).unwrap())
}

#[test]
fn discovers_only_prefixed_components() {
    let core = MockCore::start();
    let mut q = connect(&core);
    let found = q.list(PREFIX).unwrap();
    let names: Vec<&str> = found.iter().map(|d| d.component.name.as_str()).collect();
    // Case does not matter; Other_ Gain is left out.
    assert_eq!(names, ["NEST_Panel", "NEST_Zone1", "Nest_Combo"]);
    assert_eq!(found[1].component.kind.as_deref(), Some("gain"));
    let gain = found[1].controls.iter().find(|c| c.name == "gain").unwrap();
    assert_eq!(pick(gain).0, Widget::Dial { min: -100.0, max: 20.0 });
}

#[test]
fn fills_in_combo_box_choices_left_out_of_get_controls() {
    let core = MockCore::start();
    let mut q = connect(&core);
    let found = q.list(PREFIX).unwrap();
    let combo = found.iter().find(|d| d.component.name == "Nest_Combo").unwrap();
    let control = combo.controls.iter().find(|c| c.name == "Control").unwrap();
    let choices: Vec<String> = ["Mic 1", "Mic 2", "Laptop", "Streaming"].map(String::from).to_vec();
    assert_eq!(pick_for(&combo.component, control).0, Widget::Picker { choices });
    // Text controls without choices stay text entry.
    let panel = &found[0];
    let text = panel.controls.iter().find(|c| c.name == "text.1").unwrap();
    assert_eq!(pick_for(&panel.component, text).0, Widget::TextEntry);
}

#[test]
fn keeps_the_engine_status_sent_on_connect() {
    let core = MockCore::start();
    let mut q = connect(&core);
    q.list(PREFIX).unwrap();
    let notes = q.client.take_notifications();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["method"], "EngineStatus");
    assert!(q.client.take_notifications().is_empty());
}

#[test]
fn sets_and_reads_back_a_control() {
    let core = MockCore::start();
    let mut q = connect(&core);
    q.set("NEST_Zone1", "gain", json!(-20.5)).unwrap();
    q.set("NEST_Zone1", "mute", json!(true)).unwrap();
    assert_eq!(q.get("NEST_Zone1", "gain").unwrap().value, json!(-20.5));
    q.set("NEST_Panel", "text.1", json!("Hello")).unwrap();
    assert_eq!(q.get("NEST_Panel", "text.1").unwrap().string.as_deref(), Some("Hello"));
    let mute = q.get("NEST_Zone1", "mute").unwrap();
    assert!(mute.is_on());
    assert_eq!(mute.component.as_deref(), Some("NEST_Zone1"));
}

#[test]
fn reports_core_errors_with_method_and_message() {
    let core = MockCore::start();
    let mut q = connect(&core);
    match q.set("NEST_Zone1", "nope", json!(1)) {
        Err(Error::Rpc { method, code, message }) => {
            assert_eq!(method, "Component.Set");
            assert_eq!(code, 8);
            assert_eq!(message, "unknown control");
        }
        other => panic!("expected an RPC error, got {other:?}"),
    }
    // The connection is still usable after an error.
    assert_eq!(q.list(PREFIX).unwrap().len(), 3);
}

#[test]
fn polls_current_values_then_only_changes() {
    let core = MockCore::start();
    let mut q = connect(&core);
    q.subscribe("NEST_Zone1", &["gain", "mute"]).unwrap();
    // As Designer did: the first poll reports every subscribed control.
    let first = q.poll().unwrap();
    assert_eq!(first.len(), 2);
    assert!(q.poll().unwrap().is_empty());

    core.change("NEST_Zone1", "gain", json!(-6.0));
    core.change("NEST_Zone1", "invert", json!(true)); // not subscribed
    let changes = q.poll().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].component.as_deref(), Some("NEST_Zone1"));
    assert_eq!(changes[0].name, "gain");
    assert_eq!(changes[0].value, json!(-6.0));
    core.change("NEST_Zone1", "mute", json!(true));
    let changes = q.poll().unwrap();
    assert_eq!(changes[0].value, json!(1.0)); // as Designer sends a Boolean
    assert!(changes[0].is_on());
    assert!(q.poll().unwrap().is_empty());
}

#[test]
fn needs_logon_when_the_core_has_users() {
    let core = MockCore::start();
    core.require_password("1234");
    let mut q = connect(&core);
    assert!(matches!(q.list(PREFIX), Err(Error::Rpc { code: 10, .. })));
    assert!(q.client.logon("admin", "wrong").is_err());
    q.client.logon("admin", "1234").unwrap();
    assert_eq!(q.list(PREFIX).unwrap().len(), 3);
}

#[test]
fn noop_needs_no_answer() {
    let core = MockCore::start();
    let mut q = connect(&core);
    q.client.noop().unwrap();
    assert_eq!(q.list(PREFIX).unwrap().len(), 3);
}

#[test]
fn reports_a_closed_connection() {
    use std::io::Write;
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let _ = conn.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"EngineStatus\"}\0");
        // Dropped: the Core goes away.
    });
    let mut c = Client::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
    assert!(matches!(c.components(), Err(Error::Closed) | Err(Error::Io(_))));
}
