//! The round face as pixels: an upright square of 0xRRGGBB, drawn into and
//! then laid out as the panel wants it.
//!
//! The gen 3's panel is 480x640, mounted turned: the face is its first 480
//! rows, and the upright face's point (u, v) is the panel's (479 - v, u)
//! (platform/common/doom/doomgeneric_nest.c). Its pixel format is read from
//! the framebuffer at run time.

use font8x8::UnicodeFonts;

pub const FACE: usize = 480;

pub struct Canvas {
    pub size: usize,
    pub pixels: Vec<u32>,
}

/// One channel's place in a panel pixel, as fb_var_screeninfo gives it.
#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub offset: u32,
    pub length: u32,
}

/// A panel's pixel layout.
#[derive(Clone, Copy, Debug)]
pub struct PixelFormat {
    pub bytes: usize,
    pub red: Channel,
    pub green: Channel,
    pub blue: Channel,
}

impl PixelFormat {
    pub const XRGB8888: PixelFormat = PixelFormat {
        bytes: 4,
        red: Channel { offset: 16, length: 8 },
        green: Channel { offset: 8, length: 8 },
        blue: Channel { offset: 0, length: 8 },
    };
    pub const RGB565: PixelFormat = PixelFormat {
        bytes: 2,
        red: Channel { offset: 11, length: 5 },
        green: Channel { offset: 5, length: 6 },
        blue: Channel { offset: 0, length: 5 },
    };

    fn pack(&self, rgb: u32) -> u32 {
        let ch = |v: u32, c: Channel| (v >> (8 - c.length)) << c.offset;
        ch(rgb >> 16 & 0xff, self.red) | ch(rgb >> 8 & 0xff, self.green) | ch(rgb & 0xff, self.blue)
    }
}

/// The width of a character cell at scale 1.
pub const GLYPH: usize = 8;

impl Canvas {
    pub fn new(size: usize) -> Canvas {
        Canvas { size, pixels: vec![0; size * size] }
    }

    pub fn clear(&mut self, rgb: u32) {
        self.pixels.fill(rgb);
    }

    pub fn get(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.size + x]
    }

    fn put(&mut self, x: i32, y: i32, rgb: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.size && (y as usize) < self.size {
            self.pixels[y as usize * self.size + x as usize] = rgb;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, rgb);
            }
        }
    }

    /// A filled disc, or with `inner` > 0 a ring between the two radii.
    pub fn disc(&mut self, cx: i32, cy: i32, outer: f32, inner: f32, rgb: u32) {
        self.arc(cx, cy, outer, inner, 0.0, 360.0, rgb);
    }

    /// The part of a ring from `start` degrees through `sweep` degrees,
    /// clockwise, with 0 at three o'clock.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, cx: i32, cy: i32, outer: f32, inner: f32, start: f32, sweep: f32, rgb: u32) {
        let r = outer.ceil() as i32;
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let (dx, dy) = ((x - cx) as f32 + 0.5, (y - cy) as f32 + 0.5);
                let d = (dx * dx + dy * dy).sqrt();
                if d > outer || d < inner {
                    continue;
                }
                if sweep < 360.0 {
                    // Screen y runs down, so atan2 runs clockwise.
                    let a = dy.atan2(dx).to_degrees();
                    if (a - start).rem_euclid(360.0) > sweep {
                        continue;
                    }
                }
                self.put(x, y, rgb);
            }
        }
    }

    /// Blacks out everything outside the round face.
    pub fn mask_round(&mut self) {
        let c = self.size as f32 / 2.0;
        for y in 0..self.size {
            for x in 0..self.size {
                let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
                if dx * dx + dy * dy > c * c {
                    self.pixels[y * self.size + x] = 0;
                }
            }
        }
    }

    pub fn text_width(text: &str, scale: usize) -> usize {
        text.chars().count() * GLYPH * scale
    }

    /// text with its top-left at (x, y), each font pixel scale x scale.
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: usize, rgb: u32) {
        let s = scale as i32;
        for (i, ch) in text.chars().enumerate() {
            let glyph = font8x8::BASIC_FONTS
                .get(ch)
                .or_else(|| font8x8::LATIN_FONTS.get(ch))
                .unwrap_or_else(|| font8x8::BASIC_FONTS.get('?').unwrap());
            let gx = x + (i * GLYPH) as i32 * s;
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        self.fill_rect(gx + col * s, y + row as i32 * s, s, s, rgb);
                    }
                }
            }
        }
    }

    /// text centred on x, shortened with ".." to fit max_width.
    pub fn text_centred(&mut self, cx: i32, y: i32, text: &str, scale: usize, max_width: usize, rgb: u32) {
        let fit = fit(text, scale, max_width);
        let w = Canvas::text_width(&fit, scale) as i32;
        self.text(cx - w / 2, y, &fit, scale, rgb);
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.size as u32, self.size as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("writing to a Vec");
            let data: Vec<u8> =
                self.pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
            w.write_image_data(&data).expect("writing to a Vec");
        }
        out
    }

    /// The face laid out on the gen 3's panel: `stride` bytes a row, the
    /// face in its first FACE rows, turned.
    pub fn to_panel(&self, format: &PixelFormat, stride: usize) -> Vec<u8> {
        assert_eq!(self.size, FACE);
        let mut out = vec![0u8; stride * FACE];
        for y in 0..FACE {
            for x in 0..FACE {
                let (u, v) = (y, FACE - 1 - x);
                let px = format.pack(self.pixels[v * FACE + u]);
                let at = y * stride + x * format.bytes;
                out[at..at + format.bytes].copy_from_slice(&px.to_le_bytes()[..format.bytes]);
            }
        }
        out
    }
}

/// text as it fits in max_width at scale: whole, or cut with "..".
pub fn fit(text: &str, scale: usize, max_width: usize) -> String {
    let max = max_width / (GLYPH * scale);
    let n = text.chars().count();
    if n <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(2);
    text.chars().take(keep).chain("..".chars()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_outside_the_circle() {
        let mut c = Canvas::new(FACE);
        c.clear(0xffffff);
        c.mask_round();
        assert_eq!(c.get(0, 0), 0);
        assert_eq!(c.get(479, 479), 0);
        assert_eq!(c.get(240, 240), 0xffffff);
        assert_eq!(c.get(240, 2), 0xffffff);
    }

    #[test]
    fn draws_only_the_arc_asked_for() {
        let mut c = Canvas::new(100);
        // From six o'clock, a quarter turn clockwise: to nine o'clock.
        c.arc(50, 50, 40.0, 30.0, 90.0, 90.0, 1);
        assert_eq!(c.get(46, 85), 1); // just past six o'clock
        assert_eq!(c.get(25, 75), 1); // between six and nine
        assert_eq!(c.get(85, 50), 0); // three o'clock
        assert_eq!(c.get(50, 15), 0); // twelve
        assert_eq!(c.get(50, 50), 0); // inside the ring
    }

    #[test]
    fn turns_the_face_onto_the_panel() {
        let mut c = Canvas::new(FACE);
        // The face's top-left pixel, red.
        c.pixels[0] = 0xff0000;
        let stride = FACE * 4;
        let panel = c.to_panel(&PixelFormat::XRGB8888, stride);
        // (u, v) = (0, 0) is the panel's (479, 0).
        let at = 479 * 4;
        assert_eq!(&panel[at..at + 4], &[0, 0, 0xff, 0]);
        let rgb565 = c.to_panel(&PixelFormat::RGB565, FACE * 2);
        assert_eq!(&rgb565[479 * 2..479 * 2 + 2], &0xf800u16.to_le_bytes());
    }

    #[test]
    fn shortens_text_that_does_not_fit() {
        assert_eq!(fit("Hello", 2, 1000), "Hello");
        assert_eq!(fit("Hello world", 1, 48), "Hell..");
    }

    #[test]
    fn writes_a_png() {
        let png = Canvas::new(8).to_png();
        assert_eq!(&png[1..4], b"PNG");
    }
}
