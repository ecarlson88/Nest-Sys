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

/// Reads input_events into inputs: the ring's counts into whole detents
/// (`ring` keeps the part-turns), the click's presses and releases.
pub fn parse_events(bytes: &[u8], ring: &mut Ring, out: &mut Vec<Input>) {
    let w = EVENT - 8;
    for ev in bytes.chunks_exact(EVENT) {
        let kind = u16::from_ne_bytes([ev[w], ev[w + 1]]);
        let code = u16::from_ne_bytes([ev[w + 2], ev[w + 3]]);
        let value = i32::from_ne_bytes([ev[w + 4], ev[w + 5], ev[w + 6], ev[w + 7]]);
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

/// Every input device, grabbed so that nothing else sees them.
pub struct Inputs {
    files: Vec<File>,
    ring: Ring,
}

impl Inputs {
    pub fn open(dir: &Path) -> io::Result<Inputs> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(dir)?.flatten() {
            if !e.file_name().to_string_lossy().starts_with("event") {
                continue;
            }
            let Ok(f) = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(e.path()) else { continue };
            let _ = ioctl_value(&f, EVIOCGRAB, 1);
            files.push(f);
        }
        if files.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("{}: no event devices", dir.display())));
        }
        Ok(Inputs { files, ring: Ring::default() })
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
        for (f, p) in self.files.iter_mut().zip(&fds) {
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            while let Ok(got) = f.read(&mut buf) {
                if got == 0 {
                    break;
                }
                parse_events(&buf[..got - got % EVENT], &mut self.ring, &mut out);
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
        let mut out = Vec::new();
        parse_events(&bytes, &mut ring, &mut out);
        assert_eq!(out, [Input::Turn(1), Input::Down, Input::Up, Input::Turn(1), Input::Turn(-2)]);
    }

    #[test]
    fn sizes_events_for_the_platform() {
        assert_eq!(EVENT, if cfg!(target_pointer_width = "32") { 16 } else { 24 });
    }
}
