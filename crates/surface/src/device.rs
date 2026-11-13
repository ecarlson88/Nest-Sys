//! The unit's own display and ring, as the Doom port drives them
//! (platform/common/doom/doomgeneric_nest.c): the framebuffer `/dev/fb0`,
//! two pages flipped by panning, the console kept out of the way, and the
//! ring and click grabbed from `/dev/input` so nlclient sees none of them.
//! What was on the display is put back on the way out.
//!
//! Written from the Doom port and the kernel's interfaces; not yet run on a
//! unit.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::canvas::{Canvas, Channel, PixelFormat, FACE};
use crate::input::Ring;
use crate::runner::Input;

// linux/fb.h
const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOPUT_VSCREENINFO: u32 = 0x4601;
const FBIOGET_FSCREENINFO: u32 = 0x4602;
const FBIOPAN_DISPLAY: u32 = 0x4606;
const FBIOBLANK: u32 = 0x4611;
const FB_BLANK_UNBLANK: libc::c_ulong = 0;
// linux/kd.h
const KDSETMODE: u32 = 0x4B3A;
const KDGETMODE: u32 = 0x4B3B;
const KD_GRAPHICS: libc::c_ulong = 1;
// linux/input.h: _IOW('E', 0x90, int)
const EVIOCGRAB: u32 = 0x4004_4590;
/// _IOR('E', 0x06, len): the device's name.
const fn eviocgname(len: u32) -> u32 {
    0x8000_0000 | len << 16 | 0x4506
}
/// _IOR('E', 0x20 + ev, len): the codes of type ev it reports (0: its types).
const fn eviocgbit(ev: u16, len: u32) -> u32 {
    0x8000_0000 | len << 16 | (0x4520 + ev as u32)
}
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
/// The gen 2's ring.
const REL_X: u16 = 0x00;
/// The gen 3's ring.
const REL_DIAL: u16 = 0x07;
/// evdev's KEY_ENTER: the gen 3's click.
const KEY_ENTER: u16 = 28;
/// evdev's KEY_POWER: the gen 2's.
const KEY_POWER: u16 = 116;

fn ioctl<T>(fd: &impl AsRawFd, request: u32, arg: *mut T) -> io::Result<()> {
    // SAFETY: the request and argument types match linux/fb.h, kd.h and
    // input.h for every call site.
    let r = unsafe { libc::ioctl(fd.as_raw_fd(), request as libc::Ioctl, arg) };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// An ioctl whose argument is a plain number, not a pointer.
fn ioctl_value(fd: &impl AsRawFd, request: u32, arg: libc::c_ulong) -> io::Result<()> {
    // SAFETY: as ioctl; these requests take their argument by value.
    let r = unsafe { libc::ioctl(fd.as_raw_fd(), request as libc::Ioctl, arg) };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// struct fb_var_screeninfo.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct VarInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

/// struct fb_fix_screeninfo.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FixInfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    kind: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

/// What the framebuffer reports about itself, as lines for a log: what
/// --diag prints first.
pub fn describe(path: &Path) -> io::Result<String> {
    let file = OpenOptions::new().read(true).open(path)?;
    describe_file(&file)
}

fn describe_file(file: &File) -> io::Result<String> {
    let mut var = VarInfo::default();
    ioctl(file, FBIOGET_VSCREENINFO, &mut var)?;
    let mut fix = FixInfo::default();
    ioctl(file, FBIOGET_FSCREENINFO, &mut fix)?;
    let id = String::from_utf8_lossy(fix.id.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
    let ch = |b: Bitfield| format!("{}+{}{}", b.offset, b.length, if b.msb_right != 0 { " msb-right" } else { "" });
    let pages = var.yres_virtual / var.yres.max(1);
    let gray = if var.grayscale != 0 { " (grayscale)" } else { "" };
    Ok([
        format!(
            "id \"{id}\", visible {}x{}, virtual {}x{} ({pages} page(s)), shown from {},{}",
            var.xres, var.yres, var.xres_virtual, var.yres_virtual, var.xoffset, var.yoffset
        ),
        format!(
            "{} bits a pixel{gray}: red {}, green {}, blue {}, alpha {}",
            var.bits_per_pixel,
            ch(var.red),
            ch(var.green),
            ch(var.blue),
            ch(var.transp)
        ),
        format!(
            "stride {} bytes, memory {} bytes, pan step x {} y {}, wrap step {}, visual {}, type {}",
            fix.line_length, fix.smem_len, fix.xpanstep, fix.ypanstep, fix.ywrapstep, fix.visual, fix.kind
        ),
        format!(
            "physical {}x{} mm, rotate {}, nonstd {}, pixclock {} ps, vmode {}",
            var.width, var.height, var.rotate, var.nonstd, var.pixclock, var.vmode
        ),
    ]
    .join("\n"))
}

/// The backlights in /sys/class/backlight, as lines for a log.
pub fn describe_backlights() -> String {
    let Ok(dir) = std::fs::read_dir("/sys/class/backlight") else { return "no /sys/class/backlight".into() };
    let mut lines: Vec<String> = dir
        .flatten()
        .map(|e| {
            let read =
                |name: &str| std::fs::read_to_string(e.path().join(name)).map_or("?".into(), |s| s.trim().to_owned());
            let name = e.file_name().to_string_lossy().into_owned();
            format!("{name}: brightness {} of {}, power {}", read("brightness"), read("max_brightness"), read("bl_power"))
        })
        .collect();
    if lines.is_empty() {
        lines.push("no backlights".into());
    }
    lines.sort();
    lines.join("\n")
}

/// How the face sits on the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Panel {
    /// The gen 3's 480x640, turned (canvas::Canvas::to_panel_into).
    Turned,
    /// A square panel the face's size, upright.
    Square,
}

pub struct Framebuffer {
    file: File,
    var: VarInfo,
    map: *mut u8,
    len: usize,
    stride: usize,
    panel: Panel,
    format: PixelFormat,
    pages: u32,
    page: u32,
    /// The page shown before, and its place, to put back.
    before: Option<(u32, Vec<u8>)>,
    /// A frame laid out for the panel before it is copied in.
    buf: Vec<u8>,
}

impl Framebuffer {
    pub fn open(path: &Path) -> io::Result<Framebuffer> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut var = VarInfo::default();
        ioctl(&file, FBIOGET_VSCREENINFO, &mut var)?;
        let panel = match (var.xres, var.yres) {
            (480, 640) => Panel::Turned,
            (x, y) if x == y && x as usize == FACE => Panel::Square,
            (x, y) => {
                let what = format!("{}: {x}x{y}, neither the gen 3's 480x640 nor a {FACE}-square face", path.display());
                return Err(io::Error::new(io::ErrorKind::Unsupported, what));
            }
        };
        if ![16, 24, 32].contains(&var.bits_per_pixel) {
            let what = format!("{}: {} bits a pixel", path.display(), var.bits_per_pixel);
            return Err(io::Error::new(io::ErrorKind::Unsupported, what));
        }
        // Two pages, where the panel can pan between them.
        if var.yres_virtual < 2 * var.yres {
            let mut two = var;
            two.yres_virtual = 2 * var.yres;
            two.yoffset = 0;
            if ioctl(&file, FBIOPUT_VSCREENINFO, &mut two).is_ok() {
                ioctl(&file, FBIOGET_VSCREENINFO, &mut var)?;
            }
        }
        let mut fix = FixInfo::default();
        ioctl(&file, FBIOGET_FSCREENINFO, &mut fix)?;
        let stride = fix.line_length as usize;
        let len = stride * var.yres_virtual as usize;
        // SAFETY: maps the device's memory, len bytes as it reports them.
        let map = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, file.as_raw_fd(), 0)
        };
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let map = map as *mut u8;
        let page_len = stride * var.yres as usize;
        let before = (var.yoffset + var.yres <= var.yres_virtual).then(|| {
            // SAFETY: the page lies inside the mapping.
            let shown = unsafe { std::slice::from_raw_parts(map.add(var.yoffset as usize * stride), page_len) };
            (var.yoffset, shown.to_vec())
        });
        // SAFETY: the whole mapping.
        unsafe { std::ptr::write_bytes(map, 0, len) };
        let ch = |b: Bitfield| Channel { offset: b.offset, length: b.length };
        let format = PixelFormat {
            bytes: var.bits_per_pixel as usize / 8,
            red: ch(var.red),
            green: ch(var.green),
            blue: ch(var.blue),
        };
        let pages = if var.yres_virtual >= 2 * var.yres { 2 } else { 1 };
        let _ = ioctl_value(&file, FBIOBLANK, FB_BLANK_UNBLANK);
        light_up();
        let page = var.yoffset / var.yres.max(1);
        Ok(Framebuffer { file, var, map, len, stride, panel, format, pages, page, before, buf: vec![0; stride * FACE] })
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    /// As describe, once open (with the pages it set up).
    pub fn describe(&self) -> io::Result<String> {
        describe_file(&self.file)
    }

    pub fn pages(&self) -> u32 {
        self.pages
    }

    /// Shows the canvas: drawn into the page not shown, then panned to.
    pub fn show(&mut self, canvas: &Canvas) {
        match self.panel {
            Panel::Turned => canvas.to_panel_into(&self.format, self.stride, &mut self.buf),
            Panel::Square => canvas.to_upright_into(&self.format, self.stride, &mut self.buf),
        }
        let back = if self.pages == 2 { 1 - self.page.min(1) } else { 0 };
        let at = back as usize * self.var.yres as usize * self.stride;
        // SAFETY: the face's rows of one page lie inside the mapping.
        unsafe { std::ptr::copy_nonoverlapping(self.buf.as_ptr(), self.map.add(at), self.buf.len()) };
        if self.pages == 2 {
            self.var.yoffset = back * self.var.yres;
            if ioctl(&self.file, FBIOPAN_DISPLAY, &mut self.var).is_ok() {
                self.page = back;
            }
        }
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        if let Some((y, shown)) = self.before.take() {
            // SAFETY: the page it came from.
            unsafe { std::ptr::copy_nonoverlapping(shown.as_ptr(), self.map.add(y as usize * self.stride), shown.len()) };
            self.var.yoffset = y;
            let _ = ioctl(&self.file, FBIOPAN_DISPLAY, &mut self.var);
        }
        // SAFETY: the mapping made in open.
        unsafe { libc::munmap(self.map as *mut libc::c_void, self.len) };
    }
}

/// The backlight up, if whatever ran before left it off.
fn light_up() {
    let Ok(dir) = std::fs::read_dir("/sys/class/backlight") else { return };
    for e in dir.flatten() {
        let read = |name: &str| std::fs::read_to_string(e.path().join(name)).ok().and_then(|s| s.trim().parse::<u32>().ok());
        if read("brightness") != Some(0) {
            continue;
        }
        let max = read("max_brightness").filter(|&m| m > 0).unwrap_or(1);
        let _ = std::fs::write(e.path().join("brightness"), format!("{max}\n"));
    }
}

/// The console in graphics mode, so its text stays off the panel; its mode
/// before is put back when dropped.
pub struct Console {
    tty: Option<(File, libc::c_int)>,
}

impl Console {
    pub fn graphics() -> Console {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty0").ok().and_then(|f| {
            let mut mode: libc::c_int = 0;
            ioctl(&f, KDGETMODE, &mut mode).ok()?;
            ioctl_value(&f, KDSETMODE, KD_GRAPHICS).ok()?;
            Some((f, mode))
        });
        Console { tty }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if let Some((f, mode)) = &self.tty {
            let _ = ioctl_value(f, KDSETMODE, *mode as libc::c_ulong);
        }
    }
}

/// The size of a struct input_event: two longs of time, then type, code
/// and value (16 bytes on the units' 32-bit ARM).
const EVENT: usize = 2 * std::mem::size_of::<libc::c_ulong>() + 8;

/// An input_event as the kernel sent it, but for its time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawEvent {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl std::fmt::Display for RawEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let kind = match self.kind {
            EV_KEY => "EV_KEY",
            EV_REL => "EV_REL",
            0x03 => "EV_ABS",
            0x04 => "EV_MSC",
            0x05 => "EV_SW",
            _ => "type",
        };
        let code = match (self.kind, self.code) {
            (EV_REL, REL_X) => "REL_X".to_owned(),
            (EV_REL, REL_DIAL) => "REL_DIAL".to_owned(),
            (EV_KEY, KEY_ENTER) => "KEY_ENTER".to_owned(),
            (EV_KEY, KEY_POWER) => "KEY_POWER".to_owned(),
            (_, c) => format!("code {c}"),
        };
        write!(f, "{kind} ({}) {code} value {}", self.kind, self.value)
    }
}

/// Reads input_events into inputs: the ring's counts into whole detents
/// (`ring` keeps the part-turns), the click's presses and releases. Every
/// event but the reports' ends (EV_SYN) also goes to `raw`, if given.
pub fn parse_events(bytes: &[u8], ring: &mut Ring, out: &mut Vec<Input>, mut raw: Option<&mut Vec<RawEvent>>) {
    let w = EVENT - 8;
    for ev in bytes.chunks_exact(EVENT) {
        let kind = u16::from_ne_bytes([ev[w], ev[w + 1]]);
        let code = u16::from_ne_bytes([ev[w + 2], ev[w + 3]]);
        let value = i32::from_ne_bytes([ev[w + 4], ev[w + 5], ev[w + 6], ev[w + 7]]);
        if let Some(raw) = raw.as_deref_mut().filter(|_| kind != EV_SYN) {
            raw.push(RawEvent { kind, code, value });
        }
        let turned = match (kind, code) {
            (EV_REL, REL_DIAL) => ring.feed(value),
            // The gen 2's ring counts the other way round.
            (EV_REL, REL_X) => ring.feed(-value),
            (EV_KEY, KEY_ENTER | KEY_POWER) => {
                match value {
                    1 => out.push(Input::Down),
                    0 => out.push(Input::Up),
                    _ => {} // auto-repeat
                }
                0
            }
            _ => 0,
        };
        if turned != 0 {
            out.push(Input::Turn(turned));
        }
    }
}

/// An input device's name, and what it reports of the ring and click.
fn describe_input(f: &File) -> String {
    let mut name = [0u8; 64];
    let name = match ioctl(f, eviocgname(name.len() as u32), name.as_mut_ptr()) {
        Ok(()) => String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned(),
        Err(_) => "?".into(),
    };
    let bits = |ev: u16, len: usize| {
        let mut b = vec![0u8; len];
        ioctl(f, eviocgbit(ev, len as u32), b.as_mut_ptr()).ok().map(|()| b)
    };
    let has = |b: &Option<Vec<u8>>, code: u16| {
        b.as_ref().and_then(|b| b.get(code as usize / 8)).is_some_and(|x| x >> (code % 8) & 1 == 1)
    };
    let (types, rel, keys) = (bits(0, 4), bits(EV_REL, 2), bits(EV_KEY, 96));
    let mut what = Vec::new();
    for (t, label) in [(EV_KEY, "keys"), (EV_REL, "relative"), (0x03, "absolute"), (0x05, "switches")] {
        if has(&types, t) {
            what.push(label);
        }
    }
    let known = [(&rel, REL_DIAL, "REL_DIAL"), (&rel, REL_X, "REL_X"), (&keys, KEY_ENTER, "KEY_ENTER"), (&keys, KEY_POWER, "KEY_POWER")];
    for (b, code, label) in known {
        if has(b, code) {
            what.push(label);
        }
    }
    format!("\"{name}\": {}", if what.is_empty() { "nothing known".into() } else { what.join(", ") })
}

/// Every input device, grabbed so that nothing else sees them.
pub struct Inputs {
    files: Vec<File>,
    /// Each open file's place in names.
    index: Vec<usize>,
    /// Each device's file name and what it is (its name and what it
    /// reports of the ring and click), opened or not.
    pub names: Vec<(String, String)>,
    ring: Ring,
    /// The events as read, when kept (by --diag), with the device each came
    /// from (an index into names).
    pub raw: Option<Vec<(usize, RawEvent)>>,
}

impl Inputs {
    pub fn open(dir: &Path) -> io::Result<Inputs> {
        let mut found: Vec<_> = std::fs::read_dir(dir)?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
            .map(|e| e.path())
            .collect();
        found.sort();
        let (mut files, mut index, mut names) = (Vec::new(), Vec::new(), Vec::new());
        for path in found {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let f = match OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path) {
                Ok(f) => f,
                Err(e) => {
                    names.push((file_name, format!("cannot open: {e}")));
                    continue;
                }
            };
            let mut what = describe_input(&f);
            if let Err(e) = ioctl_value(&f, EVIOCGRAB, 1) {
                what.push_str(&format!(" (not grabbed: {e})"));
            }
            index.push(names.len());
            names.push((file_name, what));
            files.push(f);
        }
        if files.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("{}: no event devices", dir.display())));
        }
        Ok(Inputs { files, index, names, ring: Ring::default(), raw: None })
    }

    /// The inputs that come within timeout_ms (none if none do).
    pub fn wait(&mut self, timeout_ms: i32) -> Vec<Input> {
        let mut fds: Vec<libc::pollfd> =
            self.files.iter().map(|f| libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        // SAFETY: fds is a live array of fds.len() pollfds.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        let mut out = Vec::new();
        if n <= 0 {
            return out;
        }
        let mut buf = [0u8; EVENT * 32];
        let mut raw = Vec::new();
        for ((f, p), &device) in self.files.iter_mut().zip(&fds).zip(&self.index) {
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            while let Ok(got) = f.read(&mut buf) {
                if got == 0 {
                    break;
                }
                let keep = self.raw.is_some().then_some(&mut raw);
                parse_events(&buf[..got - got % EVENT], &mut self.ring, &mut out, keep);
                if let Some(kept) = &mut self.raw {
                    kept.extend(raw.drain(..).map(|e| (device, e)));
                }
                if got < buf.len() {
                    break;
                }
            }
        }
        out
    }
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Set once SIGTERM, SIGINT or SIGHUP arrives, so the display can be put
/// back before exiting.
pub fn stop_flag() -> &'static AtomicBool {
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: the handler only stores to an atomic.
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
    &STOP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u16, code: u16, value: i32) -> Vec<u8> {
        let mut e = vec![0u8; EVENT - 8];
        e.extend(kind.to_ne_bytes());
        e.extend(code.to_ne_bytes());
        e.extend(value.to_ne_bytes());
        e
    }

    #[test]
    fn reads_the_ring_and_the_click() {
        let mut bytes = Vec::new();
        // Clockwise is negative on the gen 3's dial: a detent and a half.
        bytes.extend(event(EV_REL, REL_DIAL, -120));
        bytes.extend(event(EV_REL, REL_DIAL, -60));
        bytes.extend(event(EV_KEY, KEY_ENTER, 1));
        bytes.extend(event(EV_KEY, KEY_ENTER, 2));
        bytes.extend(event(EV_KEY, KEY_ENTER, 0));
        bytes.extend(event(EV_REL, REL_DIAL, -60));
        bytes.extend(event(EV_REL, REL_X, -240));
        bytes.extend(event(0x03, 0x00, 5)); // something else
        let mut ring = Ring::default();
        bytes.extend(event(EV_SYN, 0, 0));
        let mut out = Vec::new();
        let mut raw = Vec::new();
        parse_events(&bytes, &mut ring, &mut out, Some(&mut raw));
        assert_eq!(out, [Input::Turn(1), Input::Down, Input::Up, Input::Turn(1), Input::Turn(-2)]);
        // All but the EV_SYN, as they came.
        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], RawEvent { kind: EV_REL, code: REL_DIAL, value: -120 });
        assert_eq!(raw[0].to_string(), "EV_REL (2) REL_DIAL value -120");
        assert_eq!(raw[7].to_string(), "EV_ABS (3) code 0 value 5");
    }

    #[test]
    fn sizes_events_for_the_platform() {
        assert_eq!(EVENT, if cfg!(target_pointer_width = "32") { 16 } else { 24 });
    }
}
