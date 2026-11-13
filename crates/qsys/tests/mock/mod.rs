//! A fake Core speaking QRC on 127.0.0.1, for tests. Its design is the one
//! Designer's emulation described in tests/data/designer (the controls of
//! Other_ Gain and Nest_Combo were not captured, so it has none); how it
//! answers Set, Get and change groups is assumed from what Designer printed.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

type Key = (String, String);

/// A change group: its controls, and those changed since its last poll.
#[derive(Default)]
struct Group {
    members: BTreeSet<Key>,
    dirty: BTreeSet<Key>,
}

#[derive(Default)]
struct State {
    /// (component, control) -> the control's GetControls description.
    controls: BTreeMap<Key, Value>,
    components: Vec<Value>,
    groups: BTreeMap<String, Group>,
    password: Option<String>,
}

pub struct MockCore {
    pub port: u16,
    state: Arc<Mutex<State>>,
}

impl MockCore {
    pub fn start() -> MockCore {
        let mut s = State::default();
        let components: Value = serde_json::from_str(include_str!("../data/designer/components.json")).unwrap();
        s.components = components.as_array().unwrap().clone();
        for fixture in [
            include_str!("../data/designer/controls_NEST_Panel.json"),
            include_str!("../data/designer/controls_NEST_Zone1.json"),
        ] {
            let list: Value = serde_json::from_str(fixture).unwrap();
            let name = list["Name"].as_str().unwrap();
            for c in list["Controls"].as_array().unwrap() {
                s.controls.insert((name.into(), c["Name"].as_str().unwrap().into()), c.clone());
            }
        }
        let state = Arc::new(Mutex::new(s));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = state.clone();
        thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let st = shared.clone();
                thread::spawn(move || serve(conn, st));
            }
        });
        MockCore { port, state }
    }

    /// Requires Logon with this password before anything else.
    pub fn require_password(&self, password: &str) {
        self.state.lock().unwrap().password = Some(password.into());
    }

    /// A change made on the Core's side (a UCI, another client, a script).
    pub fn change(&self, component: &str, control: &str, value: Value) {
        set_value(&mut self.state.lock().unwrap(), component, control, value).unwrap();
    }
}

fn set_value(s: &mut State, component: &str, control: &str, value: Value) -> Result<(), &'static str> {
    let key = (component.to_owned(), control.to_owned());
    let c = s.controls.get_mut(&key).ok_or("unknown control")?;
    c["String"] = match &value {
        Value::String(t) => Value::String(t.clone()),
        v => Value::String(v.to_string()),
    };
    // Text controls have no Value, only String.
    if c.get("Value").is_some() {
        c["Value"] = value;
    }
    for group in s.groups.values_mut() {
        if group.members.contains(&key) {
            group.dirty.insert(key.clone());
        }
    }
    Ok(())
}

fn send(conn: &mut TcpStream, msg: Value) {
    // In two writes, so a reader sees a message split across reads.
    let mut bytes = serde_json::to_vec(&msg).unwrap();
    bytes.push(0);
    let half = bytes.len() / 2;
    let _ = conn.write_all(&bytes[..half]);
    let _ = conn.flush();
    let _ = conn.write_all(&bytes[half..]);
}

fn serve(mut conn: TcpStream, state: Arc<Mutex<State>>) {
    send(&mut conn, json!({"jsonrpc": "2.0", "method": "EngineStatus",
                           "params": {"State": "Active", "DesignName": "mock", "IsEmulator": true}}));
    let mut logged_on = false;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(end) = buf.iter().position(|&b| b == 0) {
            let frame: Vec<u8> = buf.drain(..=end).collect();
            let msg: Value = serde_json::from_slice(&frame[..end]).unwrap();
            let Some(id) = msg.get("id").cloned() else { continue };
            let reply = match dispatch(&mut state.lock().unwrap(), &msg, &mut logged_on) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err((code, message)) => json!({"jsonrpc": "2.0", "id": id,
                                               "error": {"code": code, "message": message}}),
            };
            send(&mut conn, reply);
        }
    }
}

fn dispatch(s: &mut State, msg: &Value, logged_on: &mut bool) -> Result<Value, (i64, &'static str)> {
    let p = &msg["params"];
    let method = msg["method"].as_str().unwrap_or("");
    if method == "Logon" {
        if s.password.as_deref().is_some_and(|pw| p["Password"] != pw) {
            return Err((10, "Logon required"));
        }
        *logged_on = true;
        return Ok(json!(true));
    }
    if s.password.is_some() && !*logged_on {
        return Err((10, "Logon required"));
    }
    let name = p["Name"].as_str().unwrap_or("").to_owned();
    match method {
        "Component.GetComponents" => Ok(Value::Array(s.components.clone())),
        "Component.GetControls" => {
            if !s.components.iter().any(|c| c["Name"] == name) {
                return Err((8, "unknown component"));
            }
            let controls: Vec<Value> =
                s.controls.iter().filter(|((c, _), _)| *c == name).map(|(_, v)| v.clone()).collect();
            Ok(json!({"Name": name, "Controls": controls}))
        }
        "Component.Get" => {
            let mut out = Vec::new();
            for c in p["Controls"].as_array().ok_or((-32602, "bad params"))? {
                let key = (name.clone(), c["Name"].as_str().unwrap_or("").to_owned());
                let v = s.controls.get(&key).ok_or((8, "unknown control"))?;
                out.push(json!({"Name": key.1, "Value": v["Value"], "String": v["String"]}));
            }
            Ok(json!({"Name": name, "Controls": out}))
        }
        "Component.Set" => {
            for c in p["Controls"].as_array().ok_or((-32602, "bad params"))? {
                let control = c["Name"].as_str().unwrap_or("");
                set_value(s, &name, control, c["Value"].clone()).map_err(|m| (8, m))?;
            }
            Ok(json!(true))
        }
        "ChangeGroup.AddComponentControl" => {
            let id = p["Id"].as_str().unwrap_or("").to_owned();
            let comp = p["Component"]["Name"].as_str().unwrap_or("").to_owned();
            let group = s.groups.entry(id.clone()).or_default();
            let mut count = 0;
            for c in p["Component"]["Controls"].as_array().ok_or((-32602, "bad params"))? {
                let key = (comp.clone(), c["Name"].as_str().unwrap_or("").to_owned());
                // A control's first poll reports its current value.
                group.members.insert(key.clone());
                group.dirty.insert(key);
                count += 1;
            }
            Ok(json!({"Id": id, "Count": count}))
        }
        "ChangeGroup.Poll" => {
            let id = p["Id"].as_str().unwrap_or("").to_owned();
            let dirty = s.groups.get_mut(&id).map(|g| std::mem::take(&mut g.dirty)).unwrap_or_default();
            let changes: Vec<Value> = dirty
                .into_iter()
                .map(|key| {
                    let v = &s.controls[&key];
                    json!({"Component": key.0, "Name": key.1, "Value": v["Value"], "String": v["String"]})
                })
                .collect();
            Ok(json!({"Id": id, "Changes": changes}))
        }
        _ => Err((-32601, "Method not found")),
    }
}
