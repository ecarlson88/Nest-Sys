//! The runner against a fake Core (qsys's own, crates/qsys/tests/mock):
//! connecting, losing the Core and coming back, keeping the settings, and
//! never holding up the ring while the network is slow.

#[path = "../../qsys/tests/mock/mod.rs"]
mod mock;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mock::MockCore;
use surface::app::Mode;
use surface::runner::{load_settings, Config, Input, Runner};

/// A settings file of the test's own, gone before it starts.
fn settings_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("surface-runner-{}", std::process::id()));
    let path = dir.join(format!("{name}.json"));
    let _ = std::fs::remove_file(&path);
    path
}

fn runner(port: u16, settings: &Path) -> Runner {
    let mut config = Config::new("127.0.0.1", settings.to_str().unwrap());
    config.port = port;
    Runner::new(config)
}

/// Steps the runner, as a front end's loop would, until `done` or 10 s.
fn step_until(r: &mut Runner, what: &str, done: impl Fn(&Runner) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done(r) {
        assert!(Instant::now() < until, "timed out waiting until {what}");
        r.step();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Inputs, each followed by a step, as the ring and click give them.
fn inputs(r: &mut Runner, inputs: &[Input]) {
    for &i in inputs {
        r.input(i);
        r.step();
    }
}

/// Holds the click long enough to count as a hold.
fn hold(r: &mut Runner) {
    r.input(Input::Down);
    r.step();
    std::thread::sleep(Duration::from_millis(surface::input::HOLD_MS + 50));
    r.step();
    r.input(Input::Up);
    r.step();
}

fn screen_name(r: &Runner) -> Option<String> {
    r.app().and_then(|a| a.current()).map(|s| s.component.name.clone())
}

#[test]
fn connects_and_shows_the_design() {
    let core = MockCore::start();
    let mut r = runner(core.port, &settings_file("connects"));
    assert!(r.step(), "the first step draws Connecting…");
    step_until(&mut r, "connected", Runner::connected);
    let app = r.app().unwrap();
    let names: Vec<&str> = app.screens.iter().map(|s| s.component.name.as_str()).collect();
    assert_eq!(names, ["NEST_Panel", "NEST_Zone1", "Nest_Combo"]);
}

#[test]
fn follows_changes_made_on_the_core() {
    let core = MockCore::start();
    let mut r = runner(core.port, &settings_file("follows"));
    step_until(&mut r, "connected", Runner::connected);
    core.change("NEST_Zone1", "mute", serde_json::json!(true));
    step_until(&mut r, "the mute shows", |r| {
        let zone = r.app().unwrap().screens.iter().find(|s| s.component.name == "NEST_Zone1").unwrap();
        zone.items.iter().any(|i| i.control.name == "mute" && i.control.is_on())
    });
}

#[test]
fn comes_back_to_the_same_screen_when_the_core_returns() {
    let core = MockCore::start();
    let mut r = runner(core.port, &settings_file("reconnects"));
    step_until(&mut r, "connected", Runner::connected);
    // The list, one down, opened: the second screen.
    hold(&mut r);
    inputs(&mut r, &[Input::Turn(1), Input::Down, Input::Up]);
    assert_eq!(screen_name(&r).as_deref(), Some("NEST_Zone1"));

    core.drop_connections();
    step_until(&mut r, "the loss is seen", |r| !r.connected());
    step_until(&mut r, "connected again", Runner::connected);
    assert_eq!(screen_name(&r).as_deref(), Some("NEST_Zone1"));
}

#[test]
fn keeps_settings_chosen_on_the_screen() {
    let core = MockCore::start();
    let file = settings_file("settings");
    let mut r = runner(core.port, &file);
    step_until(&mut r, "connected", Runner::connected);
    assert!(load_settings(file.to_str().unwrap()).hints, "helper text is on until switched off");
    // The list, back one to Settings, its first row: helper text off.
    hold(&mut r);
    inputs(&mut r, &[Input::Turn(-1), Input::Down, Input::Up]);
    assert_eq!(r.app().unwrap().mode, Mode::Settings { focus: 0 });
    inputs(&mut r, &[Input::Down, Input::Up]);
    assert!(!load_settings(file.to_str().unwrap()).hints, "saved as soon as chosen");

    // A runner started afresh takes them up.
    drop(r);
    let mut again = runner(core.port, &file);
    step_until(&mut again, "connected", Runner::connected);
    assert!(!again.app().unwrap().settings.hints);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn keeps_stepping_while_a_core_does_not_answer() {
    // Takes the connection (the kernel does, unasked) but never answers:
    // discovery waits out its 3 s timeout.
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut r = runner(silent.local_addr().unwrap().port(), &settings_file("silent"));
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        let t = Instant::now();
        r.input(Input::Turn(1));
        r.step();
        assert!(t.elapsed() < Duration::from_millis(100), "a step took {:?}", t.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!r.connected());
}
