//! The round face as pixels: an upright square of 0xRRGGBB, drawn into with
//! smooth (anti-aliased) shapes and Inter text, then laid out as the panel
//! wants it.
//!
//! The gen 3's panel is 480x640, mounted turned: the face is its first 480
//! rows, and the upright face's point (u, v) is the panel's (479 - v, u)
//! (platform/common/doom/doomgeneric_nest.c). Its pixel format is read from
//! the framebuffer at run time.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

pub const FACE: usize = 480;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Light,
    Regular,
    Bold,
}

/// Inter at three weights (fonts/README.md), with the glyphs drawn so far.
pub struct Fonts {
    light: Font,
    regular: Font,
    bold: Font,
    glyphs: HashMap<(Weight, char, u32), (Metrics, Vec<u8>)>,
}

impl Fonts {
    fn load(bytes: &[u8]) -> Font {
        Font::from_bytes(bytes, FontSettings::default()).expect("the bundled font parses")
    }

    pub fn new() -> Fonts {
        Fonts {
            light: Fonts::load(include_bytes!("../fonts/Inter-300.ttf")),
            regular: Fonts::load(include_bytes!("../fonts/Inter-400.ttf")),
            bold: Fonts::load(include_bytes!("../fonts/Inter-600.ttf")),
            glyphs: HashMap::new(),
        }
    }

    fn font(&self, w: Weight) -> &Font {
        match w {
            Weight::Light => &self.light,
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }

    fn glyph(&mut self, w: Weight, ch: char, size: f32) -> &(Metrics, Vec<u8>) {
        let key = (w, ch, size.to_bits());
        if !self.glyphs.contains_key(&key) {
            // Animated sizes add up; start afresh now and then.
            if self.glyphs.len() > 4000 {
                self.glyphs.clear();
            }
            let font = self.font(w);
            // A character Inter lacks draws as '?'.
            let ch = if font.has_glyph(ch) { ch } else { '?' };
            let g = font.rasterize(ch, size);
            self.glyphs.insert(key, g);
        }
        &self.glyphs[&key]
    }

    /// The advance of text at size, kerning included.
    pub fn width(&self, text: &str, size: f32, w: Weight) -> f32 {
        let font = self.font(w);
        let mut x = 0.0;
        let mut prev = None;
        for ch in text.chars() {
            if let Some(p) = prev {
                x += font.horizontal_kern(p, ch, size).unwrap_or(0.0);
            }
            x += font.metrics(ch, size).advance_width;
            prev = Some(ch);
        }
        x
    }

    /// text as it fits in max_width: whole, or cut short with an ellipsis.
    pub fn fit(&self, text: &str, size: f32, w: Weight, max_width: f32) -> String {
        if self.width(text, size, w) <= max_width {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        for keep in (0..chars.len()).rev() {
            let cut: String = chars[..keep].iter().collect::<String>().trim_end().to_owned() + "…";
            if self.width(&cut, size, w) <= max_width {
                return cut;
            }
        }
        "…".into()
    }

    /// text broken into lines no wider than max_width, at most max_lines
    /// (the last cut short if need be).
    pub fn wrap(&self, text: &str, size: f32, w: Weight, max_width: f32, max_lines: usize) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if self.width(&candidate, size, w) <= max_width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        if lines.len() > max_lines {
            let rest = lines[max_lines - 1..].join(" ");
            lines.truncate(max_lines - 1);
            lines.push(rest);
        }
        lines.into_iter().map(|l| self.fit(&l, size, w, max_width)).collect()
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

pub struct Canvas {
    pub size: usize,
    pub pixels: Vec<u32>,
    pub fonts: Fonts,
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

/// rgb at a fraction a of the way from under to over.
pub fn mix(under: u32, over: u32, a: f32) -> u32 {
    let a = a.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (u, o) = ((under >> s & 0xff) as f32, (over >> s & 0xff) as f32);
        ((u + (o - u) * a).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// How much of a pixel lies inside an edge `d` pixels away (negative
/// inside): the anti-aliasing of every shape.
fn cover(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

impl Canvas {
    pub fn new(size: usize) -> Canvas {
        Canvas { size, pixels: vec![0; size * size], fonts: Fonts::new() }
    }

    pub fn clear(&mut self, rgb: u32) {
        self.pixels.fill(rgb);
    }

    pub fn get(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.size + x]
    }

    fn blend(&mut self, x: i32, y: i32, rgb: u32, a: f32) {
        if a <= 0.0 || x < 0 || y < 0 || x as usize >= self.size || y as usize >= self.size {
            return;
        }
        let i = y as usize * self.size + x as usize;
        self.pixels[i] = if a >= 1.0 { rgb } else { mix(self.pixels[i], rgb, a) };
    }

    /// Fills the pixels around (cx, cy) within r with the coverage `shape`
    /// gives each pixel centre's offset from (cx, cy).
    fn shade(&mut self, cx: f32, cy: f32, r: f32, rgb: u32, shape: impl Fn(f32, f32) -> f32) {
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let a = shape(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                self.blend(x, y, rgb, a);
            }
        }
    }

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, rgb: u32) {
        self.shade(cx, cy, r, rgb, |dx, dy| cover((dx * dx + dy * dy).sqrt() - r));
    }

    /// A circle's outline, `width` wide, centred on radius r.
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, rgb: u32) {
        let h = width / 2.0;
        self.shade(cx, cy, r + h, rgb, |dx, dy| cover(((dx * dx + dy * dy).sqrt() - r).abs() - h));
    }

    /// An arc of radius r, `width` wide with round ends, from `start`
    /// degrees through `sweep` degrees clockwise (0 at three o'clock).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, width: f32, start: f32, sweep: f32, rgb: u32) {
        if sweep <= 0.0 {
            return;
        }
        if sweep >= 360.0 {
            return self.ring(cx, cy, r, width, rgb);
        }
        let h = width / 2.0;
        let (a0, a1) = (start.to_radians(), (start + sweep).to_radians());
        let ends = [(r * a0.cos(), r * a0.sin()), (r * a1.cos(), r * a1.sin())];
        self.shade(cx, cy, r + h, rgb, |dx, dy| {
            // Screen y runs down, so atan2 runs clockwise.
            let a = dy.atan2(dx).to_degrees();
            let d = if (a - start).rem_euclid(360.0) <= sweep {
                ((dx * dx + dy * dy).sqrt() - r).abs()
            } else {
                ends.iter().map(|(ex, ey)| ((dx - ex).powi(2) + (dy - ey).powi(2)).sqrt()).fold(f32::MAX, f32::min)
            };
            cover(d - h)
        });
    }

    /// A line from (x0, y0) to (x1, y1), `width` wide with round ends; with
    /// x0 < x1 and a width as tall as wanted, a pill.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, rgb: u32) {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (vx, vy) = (x1 - x0, y1 - y0);
        let len2 = (vx * vx + vy * vy).max(1e-6);
        let h = width / 2.0;
        let reach = (len2.sqrt() / 2.0) + h;
        self.shade(cx, cy, reach, rgb, |dx, dy| {
            let (px, py) = (dx + cx - x0, dy + cy - y0);
            let t = ((px * vx + py * vy) / len2).clamp(0.0, 1.0);
            let (qx, qy) = (px - t * vx, py - t * vy);
            cover((qx * qx + qy * qy).sqrt() - h)
        });
    }

    /// Blacks out everything outside the round face.
    pub fn mask_round(&mut self) {
        let c = self.size as f32 / 2.0;
        for y in 0..self.size {
            for x in 0..self.size {
                let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
                let a = cover((dx * dx + dy * dy).sqrt() - c);
                let i = y * self.size + x;
                self.pixels[i] = mix(0, self.pixels[i], a);
            }
        }
    }

    /// text with its baseline at y, starting at x.
    pub fn text(&mut self, x: f32, y: f32, text: &str, size: f32, w: Weight, rgb: u32) {
        let mut pen = x;
        let mut prev = None;
        for ch in text.chars() {
            if let Some(p) = prev {
                pen += self.fonts.font(w).horizontal_kern(p, ch, size).unwrap_or(0.0);
            }
            let (m, bitmap) = self.fonts.glyph(w, ch, size).clone();
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (y - m.ymin as f32 - m.height as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col] as f32 / 255.0;
                    self.blend(gx + col as i32, gy + row as i32, rgb, a);
                }
            }
            pen += m.advance_width;
            prev = Some(ch);
        }
    }

    /// text centred on (cx, cy): cy is the middle of its capital letters.
    /// Cut short with an ellipsis to fit max_width.
    #[allow(clippy::too_many_arguments)]
    pub fn text_centred(&mut self, cx: f32, cy: f32, text: &str, size: f32, w: Weight, max_width: f32, rgb: u32) {
        let fit = self.fonts.fit(text, size, w, max_width);
        let width = self.fonts.width(&fit, size, w);
        // Inter's capitals stand 0.727 of the size.
        self.text(cx - width / 2.0, cy + size * 0.727 / 2.0, &fit, size, w, rgb);
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
        c.arc(50.0, 50.0, 35.0, 10.0, 90.0, 90.0, 0xffffff);
        assert_eq!(c.get(40, 83), 0xffffff); // just past six o'clock
        assert_eq!(c.get(25, 75), 0xffffff); // between six and nine
        assert_eq!(c.get(85, 50), 0); // three o'clock
        assert_eq!(c.get(50, 15), 0); // twelve
        assert_eq!(c.get(50, 50), 0); // inside the ring
    }

    #[test]
    fn smooths_edges() {
        let mut c = Canvas::new(40);
        c.disc(20.0, 20.0, 10.0, 0xffffff);
        assert_eq!(c.get(20, 20), 0xffffff);
        assert_eq!(c.get(20, 35), 0);
        // Some pixel on the edge is part-covered, a grey.
        let partial = (0..40).any(|x| {
            let p = c.get(x, 20);
            p != 0 && p != 0xffffff
        });
        assert!(partial);
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
    fn draws_text_where_asked() {
        let mut c = Canvas::new(200);
        c.text_centred(100.0, 100.0, "Hello", 40.0, Weight::Bold, 200.0, 0xffffff);
        let lit = |y0: usize, y1: usize| (y0..y1).flat_map(|y| (0..200).map(move |x| (x, y))).any(|(x, y)| c.get(x, y) != 0);
        assert!(lit(90, 110));
        assert!(!lit(0, 70) && !lit(130, 200));
    }

    #[test]
    fn shortens_and_wraps_text() {
        let f = Fonts::new();
        assert_eq!(f.fit("Hello", 20.0, Weight::Regular, 1000.0), "Hello");
        let cut = f.fit("Hello world, this is long", 20.0, Weight::Regular, 100.0);
        assert!(cut.ends_with('…') && f.width(&cut, 20.0, Weight::Regular) <= 100.0, "{cut}");
        let lines = f.wrap("one two three four five six seven", 20.0, Weight::Regular, 90.0, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }

    #[test]
    fn writes_a_png() {
        let png = Canvas::new(8).to_png();
        assert_eq!(&png[1..4], b"PNG");
    }
}
