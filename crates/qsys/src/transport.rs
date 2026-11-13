//! What the screen needs from Q-SYS, kept apart from how it is reached, so
//! a transport other than QRC (a plugin's, say) can stand in later.

use std::io::{Read, Write};

use serde_json::Value;

use crate::client::{Change, Client, Result};
use crate::control::{Component, Control};

/// A component found by discovery, with its controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Discovered {
    pub component: Component,
    pub controls: Vec<Control>,
}

pub trait Transport {
    /// The components whose Code Name starts with prefix, in the Core's
    /// order, with their controls.
    fn list(&mut self, prefix: &str) -> Result<Vec<Discovered>>;
    /// One control's current state.
    fn get(&mut self, component: &str, control: &str) -> Result<Change>;
    fn set(&mut self, component: &str, control: &str, value: Value) -> Result<()>;
    /// Reports changes to these controls from the next poll on.
    fn subscribe(&mut self, component: &str, controls: &[&str]) -> Result<()>;
    /// Changes to subscribed controls since the last poll.
    fn poll(&mut self) -> Result<Vec<Change>>;
}

/// The change group this transport uses (of the four a Core allows).
pub const GROUP: &str = "nest";

pub struct Qrc<S> {
    pub client: Client<S>,
}

impl<S: Read + Write> Qrc<S> {
    pub fn new(client: Client<S>) -> Self {
        Qrc { client }
    }
}

impl<S: Read + Write> Transport for Qrc<S> {
    fn list(&mut self, prefix: &str) -> Result<Vec<Discovered>> {
        let components = self.client.components()?;
        let mut found = Vec::new();
        for component in components.into_iter().filter(|c| c.name.starts_with(prefix)) {
            let controls = self.client.controls(&component.name)?;
            found.push(Discovered { component, controls });
        }
        Ok(found)
    }

    fn get(&mut self, component: &str, control: &str) -> Result<Change> {
        let method = "Component.Get";
        self.client.get(component, &[control])?.into_iter().find(|c| c.name == control).ok_or_else(|| {
            crate::client::Error::Unexpected { method: method.into(), result: Value::String(format!("no {control}")) }
        })
    }

    fn set(&mut self, component: &str, control: &str, value: Value) -> Result<()> {
        self.client.set(component, control, value)
    }

    fn subscribe(&mut self, component: &str, controls: &[&str]) -> Result<()> {
        self.client.change_group_add(GROUP, component, controls)
    }

    fn poll(&mut self) -> Result<Vec<Change>> {
        self.client.change_group_poll(GROUP)
    }
}
