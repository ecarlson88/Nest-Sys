//! The Nest's round screen as a Q-SYS control surface: one screen per
//! NEST_ component, driven by the ring and its click.
//!
//! [`app`] holds what is shown and what the ring and click do, [`render`]
//! draws it on a [`canvas::Canvas`], and [`input`] turns the ring's counts
//! and the click's presses into events. The simulator (`src/bin/sim.rs`)
//! runs it against a Core in a browser; on the unit the framebuffer and
//! input devices take the browser's place.

pub mod app;
pub mod canvas;
pub mod input;
pub mod render;
