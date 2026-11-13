//! A blocking QRC client: one TCP connection to a Core (or Designer's
//! emulation) on port 1710, requests answered in order.
//!
//! Methods follow https://help.qsys.com/Content/External_Control_APIs/QRC/QRC_Commands.htm.
//! The Core also sends messages nobody asked for (EngineStatus on connect);
//! those are kept for take_notifications.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{json, Value};

use crate::control::{Component, Control};
use crate::frame::{encode, Framer};

pub const DEFAULT_PORT: u16 = 1710;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The Core closed the connection.
    Closed,
    /// A message that is not JSON.
    Json(serde_json::Error),
    /// The Core answered with a JSON-RPC error.
    Rpc { method: String, code: i64, message: String },
    /// A result not shaped as the method promises.
    Unexpected { method: String, result: Value },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Closed => write!(f, "connection closed by the Core"),
            Error::Json(e) => write!(f, "bad message from the Core: {e}"),
            Error::Rpc { method, code, message } => write!(f, "{method}: error {code}: {message}"),
            Error::Unexpected { method, result } => write!(f, "{method}: unexpected result {result}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One control's state from Component.Get or a ChangeGroup.Poll. Unlike
/// GetControls these carry no Type: a Boolean's Value is 0.0/1.0 and a Text
/// control's Value is a meaningless 0.0 (its text is String).
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// The component, for a component control; None for a Named Control.
    pub component: Option<String>,
    pub name: String,
    pub value: Value,
    pub string: Option<String>,
    /// A combo box's options; None or empty for other controls.
    pub choices: Option<Vec<String>>,
    /// The label a design gave the control, when not empty.
    pub legend: Option<String>,
    pub disabled: bool,
    pub invisible: bool,
}

impl Change {
    pub fn from_json(v: &Value) -> Option<Change> {
        let flag = |key| v.get(key).and_then(Value::as_bool).unwrap_or(false);
        Some(Change {
            component: v.get("Component").and_then(Value::as_str).map(str::to_owned),
            name: v.get("Name")?.as_str()?.to_owned(),
            value: v.get("Value").cloned().unwrap_or(Value::Null),
            string: v.get("String").and_then(Value::as_str).map(str::to_owned),
            choices: v
                .get("Choices")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()),
            legend: v.get("Legend").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned),
            disabled: flag("Disabled"),
            invisible: flag("Invisible"),
        })
    }

    /// The value as on or off (see control::truthy).
    pub fn is_on(&self) -> bool {
        crate::control::truthy(&self.value)
    }
}

pub struct Client<S> {
    stream: S,
    framer: Framer,
    next_id: u64,
    notifications: VecDeque<Value>,
}

impl Client<TcpStream> {
    /// Connects to host:port, with timeout for the connection and for each
    /// read and write.
    pub fn connect(host: &str, port: u16, timeout: Duration) -> Result<Self> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{host}: no address"));
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(timeout))?;
                    stream.set_write_timeout(Some(timeout))?;
                    stream.set_nodelay(true)?;
                    return Ok(Client::new(stream));
                }
                Err(e) => last = e,
            }
        }
        Err(last.into())
    }
}

impl<S: Read + Write> Client<S> {
    pub fn new(stream: S) -> Self {
        Client { stream, framer: Framer::default(), next_id: 1, notifications: VecDeque::new() }
    }

    fn read_message(&mut self) -> Result<Value> {
        loop {
            if let Some(msg) = self.framer.next_message() {
                return msg.map_err(Error::Json);
            }
            let mut chunk = [0u8; 4096];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                return Err(Error::Closed);
            }
            self.framer.push(&chunk[..n]);
        }
    }

    /// Sends a request and waits for its answer, keeping whatever else
    /// arrives meanwhile as a notification.
    pub fn call(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut msg = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if let Some(p) = params {
            msg["params"] = p;
        }
        self.stream.write_all(&encode(&msg))?;
        loop {
            let reply = self.read_message()?;
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                self.notifications.push_back(reply);
                continue;
            }
            if let Some(err) = reply.get("error") {
                return Err(Error::Rpc {
                    method: method.to_owned(),
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: err.get("message").and_then(Value::as_str).unwrap_or("").to_owned(),
                });
            }
            return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Sends a message that gets no answer.
    pub fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let mut msg = json!({"jsonrpc": "2.0", "method": method});
        if let Some(p) = params {
            msg["params"] = p;
        }
        self.stream.write_all(&encode(&msg))?;
        Ok(())
    }

    /// The connection itself (its addresses, say).
    pub fn get_ref(&self) -> &S {
        &self.stream
    }

    /// Messages the Core sent unasked, oldest first.
    pub fn take_notifications(&mut self) -> Vec<Value> {
        self.notifications.drain(..).collect()
    }

    /// Needed only when the Core has users configured.
    pub fn logon(&mut self, user: &str, password: &str) -> Result<()> {
        self.call("Logon", Some(json!({"User": user, "Password": password})))?;
        Ok(())
    }

    /// Keeps the connection open; the Core drops a client silent for long.
    /// Sent as a notification, as the docs' example has no id.
    pub fn noop(&mut self) -> Result<()> {
        self.notify("NoOp", Some(json!({})))
    }

    /// Every Named Component (one with a Code Name and Script Access
    /// External or All).
    pub fn components(&mut self) -> Result<Vec<Component>> {
        // The docs' example passes a placeholder params; so does this.
        let method = "Component.GetComponents";
        let result = self.call(method, Some(json!("test")))?;
        let Value::Array(list) = &result else {
            return Err(Error::Unexpected { method: method.into(), result });
        };
        Ok(list.iter().filter_map(Component::from_json).collect())
    }

    pub fn controls(&mut self, component: &str) -> Result<Vec<Control>> {
        let method = "Component.GetControls";
        let result = self.call(method, Some(json!({"Name": component})))?;
        let Some(list) = result.get("Controls").and_then(Value::as_array) else {
            return Err(Error::Unexpected { method: method.into(), result });
        };
        Ok(list.iter().filter_map(Control::from_json).collect())
    }

    /// The current state of some of a component's controls.
    pub fn get(&mut self, component: &str, controls: &[&str]) -> Result<Vec<Change>> {
        let method = "Component.Get";
        let names: Vec<Value> = controls.iter().map(|n| json!({"Name": n})).collect();
        let result = self.call(method, Some(json!({"Name": component, "Controls": names})))?;
        let Some(list) = result.get("Controls").and_then(Value::as_array) else {
            return Err(Error::Unexpected { method: method.into(), result });
        };
        Ok(list
            .iter()
            .filter_map(Change::from_json)
            .map(|c| Change { component: Some(component.to_owned()), ..c })
            .collect())
    }

    pub fn set(&mut self, component: &str, control: &str, value: Value) -> Result<()> {
        self.call(
            "Component.Set",
            Some(json!({"Name": component, "Controls": [{"Name": control, "Value": value}]})),
        )?;
        Ok(())
    }

    /// Adds controls to change group `group` (created on first use; the
    /// Core allows four).
    pub fn change_group_add(&mut self, group: &str, component: &str, controls: &[&str]) -> Result<()> {
        let names: Vec<Value> = controls.iter().map(|n| json!({"Name": n})).collect();
        self.call(
            "ChangeGroup.AddComponentControl",
            Some(json!({"Id": group, "Component": {"Name": component, "Controls": names}})),
        )?;
        Ok(())
    }

    /// The group's controls that changed since the last poll.
    pub fn change_group_poll(&mut self, group: &str) -> Result<Vec<Change>> {
        let method = "ChangeGroup.Poll";
        let result = self.call(method, Some(json!({"Id": group})))?;
        let Some(list) = result.get("Changes").and_then(Value::as_array) else {
            return Err(Error::Unexpected { method: method.into(), result });
        };
        Ok(list.iter().filter_map(Change::from_json).collect())
    }
}
