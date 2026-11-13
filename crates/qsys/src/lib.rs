//! The Q-SYS side of the Nest control surface: a QRC client that finds the
//! running design's NEST_ components and the widget each control gets.
//!
//! Only components with a Code Name starting with [`PREFIX`] (in any case)
//! and Script Access External or All are shown; nothing about the design is
//! built in.

pub mod client;
pub mod control;
pub mod frame;
pub mod transport;
pub mod widget;

pub use client::{Change, Client, Error, Result, DEFAULT_PORT};
pub use control::{Component, Control, Direction, Kind};
pub use transport::{Discovered, Qrc, Transport};
pub use widget::{pick, pick_for, Note, Widget};

/// The Code Name prefix of the components the screen shows, in any case.
pub const PREFIX: &str = "NEST_";

/// Whether name starts with prefix, ignoring ASCII case.
pub fn has_prefix(name: &str, prefix: &str) -> bool {
    name.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_prefix_in_any_case() {
        assert!(has_prefix("NEST_Zone1", PREFIX));
        assert!(has_prefix("Nest_Combo", PREFIX));
        assert!(has_prefix("nest_x", PREFIX));
        assert!(!has_prefix("NEST", PREFIX));
        assert!(!has_prefix("Other_ Gain", PREFIX));
        assert!(!has_prefix("NESé_", PREFIX));
        assert!(has_prefix("anything", ""));
    }
}
