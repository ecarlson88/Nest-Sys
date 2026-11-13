//! The Q-SYS side of the Nest control surface: a QRC client that finds the
//! running design's NEST_ components and the widget each control gets.
//!
//! Only components with a Code Name starting with [`PREFIX`] and Script
//! Access External or All are shown; nothing about the design is built in.

pub mod client;
pub mod control;
pub mod frame;
pub mod transport;
pub mod widget;

pub use client::{Change, Client, Error, Result, DEFAULT_PORT};
pub use control::{Component, Control, Direction, Kind};
pub use transport::{Discovered, Qrc, Transport};
pub use widget::{pick, Note, Widget};

/// The Code Name prefix of the components the screen shows.
pub const PREFIX: &str = "NEST_";
