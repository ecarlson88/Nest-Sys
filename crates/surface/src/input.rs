//! The ring and its click as the screen sees them.
//!
//! On the gen 3 the ring reports `REL_DIAL` counts, 120 to a detent and
//! negative clockwise, and the click `KEY_ENTER` presses and releases
//! (platform/common/doom/doomgeneric_nest.c). These turn them into whole
//! detents and into clicks and holds.

/// Ring counts per detent.
pub const COUNTS_PER_DETENT: i32 = 120;
/// How long the click must be held to count as a hold, in milliseconds.
pub const HOLD_MS: u64 = 700;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Detents turned, positive clockwise.
    Turn(i32),
    /// The click pressed down.
    Down,
    /// The click let go.
    Up,
    /// Pressed and let go before HOLD_MS.
    Click,
    /// Held for HOLD_MS (sent once, while still held).
    Hold,
}

/// Raw ring counts to detents.
#[derive(Default)]
pub struct Ring {
    counts: i32,
}

impl Ring {
    /// Takes `REL_DIAL` counts; returns the whole detents they complete,
    /// positive clockwise.
    pub fn feed(&mut self, dial: i32) -> i32 {
        self.counts -= dial;
        let detents = self.counts / COUNTS_PER_DETENT;
        self.counts -= detents * COUNTS_PER_DETENT;
        detents
    }
}

/// The click's presses and releases to clicks and holds.
#[derive(Default)]
pub struct Button {
    down_at: Option<u64>,
    held: bool,
}

impl Button {
    pub fn down(&mut self, now_ms: u64) -> Vec<Event> {
        if self.down_at.is_some() {
            return Vec::new();
        }
        self.down_at = Some(now_ms);
        self.held = false;
        vec![Event::Down]
    }

    pub fn up(&mut self, _now_ms: u64) -> Vec<Event> {
        if self.down_at.take().is_none() {
            return Vec::new();
        }
        if self.held {
            vec![Event::Up]
        } else {
            vec![Event::Up, Event::Click]
        }
    }

    /// A Hold once the click has been down for HOLD_MS.
    pub fn tick(&mut self, now_ms: u64) -> Option<Event> {
        let at = self.down_at?;
        if !self.held && now_ms.saturating_sub(at) >= HOLD_MS {
            self.held = true;
            return Some(Event::Hold);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_whole_detents_clockwise_positive() {
        let mut r = Ring::default();
        assert_eq!(r.feed(-60), 0);
        assert_eq!(r.feed(-60), 1);
        assert_eq!(r.feed(-360), 3);
        assert_eq!(r.feed(240), -2);
        assert_eq!(r.feed(100), 0);
        assert_eq!(r.feed(-100), 0);
    }

    #[test]
    fn tells_a_click_from_a_hold() {
        let mut b = Button::default();
        assert_eq!(b.down(0), [Event::Down]);
        assert_eq!(b.tick(100), None);
        assert_eq!(b.up(200), [Event::Up, Event::Click]);

        assert_eq!(b.down(1000), [Event::Down]);
        assert_eq!(b.tick(1000 + HOLD_MS), Some(Event::Hold));
        assert_eq!(b.tick(5000), None);
        assert_eq!(b.up(5000), [Event::Up]);
    }

    #[test]
    fn ignores_repeated_presses_and_stray_releases() {
        let mut b = Button::default();
        assert!(b.up(0).is_empty());
        b.down(0);
        assert!(b.down(10).is_empty());
    }
}
