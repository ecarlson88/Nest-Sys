//! The first thing to run on a unit (`nest-surface --diag`): a test pattern
//! that shows whether the face sits upright, unmirrored and round on the
//! panel and how its colours come out, and a count of what the ring and
//! click do, so that their direction and feel can be checked by hand.
//!
//! This holds what is counted and how it is drawn; the unit's side is in
//! `src/bin/nest-surface.rs`.

use crate::canvas::{Canvas, Weight, FACE};
use crate::input::Event;

const C: f32 = FACE as f32 / 2.0;
const BG: u32 = 0x000000;
const FG: u32 = 0xffffff;
const DIM: u32 = 0x9a9a9a;
/// Where the ring's marker sits, and how far it moves a detent.
const MARK_R: f32 = 214.0;
const MARK_STEP: f32 = 6.0;

/// What the ring and click have done since the diagnostics started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diag {
    /// Detents, positive clockwise, all told.
    pub detents: i32,
    pub clockwise: u32,
    pub anticlockwise: u32,
    pub clicks: u32,
    pub holds: u32,
    pub pressed: bool,
    /// The last thing that happened, as the screen and the log say it.
    pub last: String,
}

impl Diag {
    /// Counts an event; returns a line for the log.
    pub fn handle(&mut self, ev: Event) -> String {
        match ev {
            Event::Turn(n) => {
                self.detents += n;
                if n > 0 {
                    self.clockwise += n as u32;
                } else {
                    self.anticlockwise += n.unsigned_abs();
                }
                let way = if n > 0 { "clockwise" } else { "anticlockwise" };
                let s = if n.abs() == 1 { "" } else { "s" };
                self.last = format!("{} detent{s} {way}", n.abs());
                format!("turn {n:+} ({way}), {:+} in all", self.detents)
            }
            Event::Down => {
                self.pressed = true;
                "press".into()
            }
            Event::Up => {
                self.pressed = false;
                "release".into()
            }
            Event::Click => {
                self.clicks += 1;
                self.last = "Click".into();
                format!("click (click {})", self.clicks)
            }
            Event::Hold => {
                self.holds += 1;
                self.last = "Hold".into();
                format!("hold (hold {})", self.holds)
            }
        }
    }

    /// The test pattern with the counts.
    pub fn draw(&self, canvas: &mut Canvas) {
        assert_eq!(canvas.size, FACE);
        canvas.clear(BG);

        // The face's edge, with a tick at each quarter: all of it should
        // be seen, as a circle, not cut off or oval.
        canvas.ring(C, C, C - 2.0, 3.0, FG);
        canvas.ring(C, C, C - 12.0, 1.0, DIM);
        for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
            canvas.line(
                C + dx * (C - 30.0),
                C + dy * (C - 30.0),
                C + dx * (C - 4.0),
                C + dy * (C - 4.0),
                3.0,
                FG,
            );
        }

        // Up, and which side is which: reads wrongly if the panel turns or
        // mirrors the face.
        canvas.line(C, 112.0, C, 52.0, 6.0, FG);
        canvas.line(C, 50.0, C - 18.0, 70.0, 6.0, FG);
        canvas.line(C, 50.0, C + 18.0, 70.0, 6.0, FG);
        canvas.text_centred(C, 132.0, "UP", 22.0, Weight::Bold, 80.0, FG);
        canvas.text_centred(52.0, C, "L", 30.0, Weight::Bold, 40.0, FG);
        canvas.text_centred(FACE as f32 - 52.0, C, "R", 30.0, Weight::Bold, 40.0, FG);

        // Colour bars, then a grey ramp in 32 steps: the panel's channel
        // order, and its depth (a 16-bit panel shows bands in the ramp).
        let bars = [
            (0xff0000, "R"),
            (0x00ff00, "G"),
            (0x0000ff, "B"),
            (0xffffff, "W"),
        ];
        let (w, x0, y0, h) = (56.0, C - 2.0 * 56.0 - 6.0, 156.0, 52.0);
        for (i, (rgb, name)) in bars.iter().enumerate() {
            let x = x0 + i as f32 * (w + 4.0);
            fill(canvas, x, y0, w, h, *rgb);
            let ink = if *rgb == 0x0000ff { FG } else { BG };
            canvas.text_centred(x + w / 2.0, y0 + h / 2.0, name, 22.0, Weight::Bold, w, ink);
        }
        let (rx, ry, rh) = (C - 128.0, 216.0, 24.0);
        for i in 0..256 {
            let v = i as u32;
            fill(canvas, rx + i as f32, ry, 1.0, rh, v << 16 | v << 8 | v);
        }

        // The ring: a marker that moves with each detent (clockwise when the
        // ring turns clockwise), then the counts.
        let a = (-90.0 + self.detents as f32 * MARK_STEP).to_radians();
        canvas.disc(C + MARK_R * a.cos(), C + MARK_R * a.sin(), 9.0, 0xffa000);
        canvas.text_centred(
            C,
            278.0,
            &format!("Ring {:+}", self.detents),
            34.0,
            Weight::Bold,
            300.0,
            FG,
        );
        let turns = format!(
            "{} clockwise · {} anticlockwise",
            self.clockwise, self.anticlockwise
        );
        canvas.text_centred(C, 316.0, &turns, 19.0, Weight::Regular, 320.0, DIM);
        let clicks = format!("{} clicks · {} holds", self.clicks, self.holds);
        canvas.text_centred(C, 346.0, &clicks, 19.0, Weight::Regular, 320.0, DIM);
        if self.pressed {
            canvas.disc(C, 392.0, 16.0, 0x30c060);
        } else if !self.last.is_empty() {
            canvas.text_centred(C, 392.0, &self.last, 22.0, Weight::Regular, 300.0, FG);
        }
        canvas.mask_round();
    }
}

fn fill(canvas: &mut Canvas, x: f32, y: f32, w: f32, h: f32, rgb: u32) {
    for py in y as usize..(y + h) as usize {
        for px in x as usize..(x + w) as usize {
            canvas.pixels[py * canvas.size + px] = rgb;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_turns_clicks_and_holds() {
        let mut d = Diag::default();
        assert_eq!(d.handle(Event::Turn(1)), "turn +1 (clockwise), +1 in all");
        d.handle(Event::Turn(2));
        assert_eq!(
            d.handle(Event::Turn(-1)),
            "turn -1 (anticlockwise), +2 in all"
        );
        assert_eq!((d.detents, d.clockwise, d.anticlockwise), (2, 3, 1));
        assert_eq!(d.last, "1 detent anticlockwise");
        d.handle(Event::Down);
        assert!(d.pressed);
        d.handle(Event::Up);
        d.handle(Event::Click);
        d.handle(Event::Hold);
        assert_eq!((d.clicks, d.holds, d.pressed), (1, 1, false));
        assert_eq!(d.last, "Hold");
    }

    #[test]
    fn draws_the_pattern_the_right_way_up() {
        let mut c = Canvas::new(FACE);
        Diag::default().draw(&mut c);
        // The arrow's shaft above the centre, the bars' red left of green.
        assert_eq!(c.get(240, 80), FG);
        assert_eq!(c.get(240, 400), BG);
        assert_eq!(c.get(x_of_bar(0), 160), 0xff0000);
        assert_eq!(c.get(x_of_bar(2), 160), 0x0000ff);
        // The ramp runs dark to light, left to right.
        assert!(c.get(120, 230) < c.get(350, 230));
        // The marker starts at twelve o'clock and moves clockwise.
        assert_eq!(c.get(240, 240 - MARK_R as usize), 0xffa000);
        let mut turned = Diag::default();
        turned.handle(Event::Turn(15));
        turned.draw(&mut c);
        assert_eq!(c.get(240 + MARK_R as usize, 240), 0xffa000);
    }

    fn x_of_bar(i: usize) -> usize {
        (C - 2.0 * 56.0 - 6.0 + i as f32 * 60.0 + 28.0) as usize
    }
}
