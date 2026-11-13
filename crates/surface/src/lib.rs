//! The Nest's round screen as a Q-SYS control surface: one screen per
//! NEST_ component, driven by the ring and its click.
//!
//! [`app`] holds what is shown and what the ring and click do, [`render`]
//! draws it on a [`canvas::Canvas`], and [`input`] turns the ring's counts
//! and the click's presses into events. [`runner`] runs it all against a
//! Core for a front end: the simulator (`src/bin/sim.rs`) shows it in a
//! browser, and `src/bin/nest-surface.rs` on the unit's framebuffer.

pub mod app;
pub mod canvas;
pub mod input;
pub mod render;
pub mod runner;
