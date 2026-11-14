# Nest Gen 3 as a Q-SYS control surface — handoff

> **New session? Start with [NEXT_SESSION.md](NEXT_SESSION.md)**: where things stand, Erik's decisions and the next steps. Parts of this file below are earlier notes; where they disagree with README.md or the code, those are current.

## Hardware session checklist

For the day the gen 3 arrives. Facts come from this repository (README.md,
`docs/integrations/`, the app's own text); **[unknown]** marks what only the
unit will tell.

**Seen on Erik's unit (2026-11-14, stock software, before rooting)**

- USB alone powers it, but the stock UI only asks to be put on its wall
  base; the backplate simulator (step 3) is meant for running off the wall
  **[not yet verified]**.
- Stock text entry, a model if the surface ever needs one: a-z laid around
  the edge, a white dot on the letter the ring has reached; shift, space and
  delete at the lower right; back, a check mark (enter) and "123" (digits
  and symbols) along the bottom; the text field in the middle, a teal pill
  (a lock icon for a password). Erik: "it does work well".
- Stock choice pages (the clock's 12/24 hour): the current value at the top,
  the choices as a list with a check mark on the one in force and a teal
  pill on the one the ring is on, DONE at the bottom.

**Before it arrives (Windows PC)**

- [ ] The unfr0zen app: the latest **upstream release** for Windows
  (`unfr0zen.exe`, github.com/unfr0zen/unfr0zen/releases). It carries the
  recovery firmware; building it here would need a firmware build, which
  Windows cannot do. Its rooting code is the same as this fork's. Its CLI is
  not on Windows: the window does everything needed.
- [ ] WSL2 with Ubuntu, for building and copying `nest-surface`: Rust
  (rustup), then zig 0.16.0 and cargo-zigbuild 0.23.4 as `BUILDING.md` and
  NEXT_SESSION.md "Tooling notes" say (zig through `pip install
  ziglang==0.16.0` in a venv works). Check with
  `sh crates/surface/build-unit.sh` -> `build/nest-surface-armv7`. No
  Docker needed: that is for the firmware build only.
- [ ] A micro-USB cable that carries data (charge-only cables are the usual
  failure). **[unknown]** how to power it on the bench for long runs
  (Open questions, below).
- [ ] An SSH key: the app's **Create a key for me** makes an ECDSA P-256
  key in `~/.ssh/unfr0zen` (Windows side), the type the gen 3's stock
  OpenSSH 6.0 takes (Ed25519 is not). Copy it into WSL (`~/.ssh`, mode 600)
  for `install-unit.sh`.

**With the unit (in this order)**

1. [ ] Note the firmware version (in its Settings menu; the exact page
   **[from memory]**: Technical info) and the serial; tested versions: 5.9.4-5, 6.3-5, 6.4-8, 6.5-14. Ideally
   keep it off Erik's Nest account.
2. [ ] Take the display off its base, plug it in, **back up the NAND**
   (app: Backups -> Back up unit; or the backup offered on the Patch page's
   Review, which uses one recovery trip for both). 512 MiB with a `.sha256`.
   Keep it outside the repository; copy it to a second place.
3. [ ] Patch with the app: on Features, **SSH** with the key (password
   login off); on Review, under Developer options, **Backplate simulator**
   for the bench (1H1C with simulated temperatures, so it does not complain off
   the wall). Nothing else is needed: no MQTT, Matter, Zigbee, Doom.
   Windows only, if it stops with `ACM interface for 18d1:4ee0 did not
   appear`: README.md "When something is wrong" has the Device Manager fix.
4. [ ] Back on its base or bench power, join Wi-Fi from its own menu
   (Settings -> Network), note its IP, then from WSL:
   `ssh -i ~/.ssh/unfr0zen root@UNIT`. **[unknown]** whether a current
   OpenSSH client needs legacy options for OpenSSH 6.0; if it refuses, the
   error names the algorithm to add (`-o KexAlgorithms=+...` or
   `-o HostKeyAlgorithms=+...`), and `SSH_OPTS` passes it to
   `install-unit.sh`.
5. [ ] Look around before changing anything: `uname -a`, `ps`,
   `ls /etc/init.d /etc/monit*`, `cat /proc/fb`, `ls -l /dev/fb* /dev/input`,
   `ls /sys/class/backlight`, `df -h /media/data`, `free`. Save the output
   for HANDOFF.md, in our own words.
6. [ ] Copy and run the diagnostics:
   `SSH_OPTS="-i ~/.ssh/unfr0zen" sh crates/surface/install-unit.sh UNIT CORE`,
   then on the unit `sh /media/data/nest-surface/nest-surface.sh --diag`.
   Check the pattern as README.md "On the unit" says (up, L/R, round, colours,
   ring direction); paste the printed report back into the session.
7. [ ] The real thing: `sh /media/data/nest-surface/nest-surface.sh` with
   Designer emulating on the PC (the unit must reach the PC's port 1710:
   allow it through the Windows firewall). Watch the frame rate, the
   hand-over (whether nlclient stays dark), and whether the unit suspends
   after a minute or two untouched **[unknown]**.
8. [ ] Recovery: confirm that **Restore to unit** with the backup would work
   (the app verifies the image first), without needing it yet.

A session on Erik's PC (Claude Code there, or `claude remote-control`) can
run steps 4 to 7 itself; a cloud session can only read pasted output.

Written 2026-11-12. Context for Claude Code working in the repo. Items are marked **[verified]** (read in docs/repos this session), **[from memory]** (recalled, not checked), or **[unknown]** (must be found out on hardware).

## Goal

Use a used Nest Learning Thermostat **Gen 3** (`diamond3`) purely as hardware: its **round display + rotary ring/click**. The end product is **not a thermostat**. The unit should **boot straight into a Q-SYS control surface** that talks **directly to a Q-SYS Core over QRC**. **No control names are hard-coded on the Nest**: it discovers controls from the running design.

No hardware yet; shopping for cheap used Gen 3 units on eBay.

## Decisions made

1. **Fork `unfr0zen/unfr0zen`** (not Fledge). Reason: the end product should boot straight into the app with the stock thermostat software removed. Fledge adds Home Assistant cards that would just be deleted. Fledge stays a *reference* for how a custom app takes over the display/ring.
2. **Direct QRC** to the Core (not a Q-SYS plugin, not via Home Assistant). Keep the Q-SYS transport behind a small interface (`get`, `set`, `subscribe`, `list`) so a plugin transport could be swapped in later.
3. **Discovery by Code Name prefix `NEST_`.** Only components whose Code Name starts with `NEST_` (and have Script Access = External or All) are shown. UI widgets are chosen from each control's type/range/direction.

## Facts

### unfr0zen — https://github.com/unfr0zen/unfr0zen  [verified]
- Roots/frees Nest Learning Thermostat gen 1/2/3 over USB; local control via MQTT, Matter (Wi-Fi/Thread), Zigbee. Desktop app + CLI.
- **GPL-3.0.** Bundled parts (U-Boot, BusyBox, doomgeneric, `zbwake` kernel module) keep their own licenses. Fork must stay GPL-3.0 if distributed.
- Repo layout: `crates/`, `platform/`, `integrations/`, `utils/` (build scripts), `assets/`, `.circleci/` (CI), `Cargo.toml`, `Dockerfile`, `.gitmodules`, `BUILDING.md`, `AGENTS.md`, `CLAUDE.md`. Read `AGENTS.md`/`CLAUDE.md` first.
- Gen 3 tested firmware: 5.9.4-5, 6.3-5, 6.4-8, 6.5-14 (others not blocked).
- Mechanism: installs a `nestlocal` service that stands in for Google's cloud; stock `nlclient` is kept with an **undo journal** so the patch can be reverted. App can **back up/restore the full NAND image** and includes recovery firmware.
- CLI has an `nlclient` command (patch stock nlclient / restore from undo journal), `--ssh` (run an SSH server on the unit), `--block-cloud`, `--doom` (optional Doom port based on doomgeneric).
- **Doom "takes over the UI"; while it runs the thermostat does not control heating/cooling.** This is the existing proof that a custom app can drive the display + ring. How it does so has **not been read** [unknown].
- USB shell/initramfs shell exists on gen 2/3 (not gen 1). Device connects to the host via the micro-USB port on the back (data cable required).
- Gen 3 flow (BUILDING.md): loads a recovery U-Boot over the ROM serial download, then boots the slot's kernel with an initramfs to patch the UBIFS root.
- Build (BUILDING.md): init submodules → `utils/build-firmware.sh` (builds firmware, nestlocal, Doom into `build/`) → `cargo build --release -p unfr0zen-gui`. First firmware build needs internet (busybox, Nest kernel/x-loader, Silicon Labs SDK, OpenThread). macOS/Linux only for firmware; Windows cannot build firmware. Linux needs `arm-none-eabi`, CMake, Ninja, clang, Rust, zig 0.16.0, cargo-zigbuild 0.23.4, Docker. macOS Docker VM needs ≥ 8 GiB RAM.

### Fledge — https://github.com/sysmoon14/fledge  [verified]
- Fork of unfr0zen adding Home Assistant screen cards, a thermostat card, own releases; GPL-3.0. Tested only on one Gen 3 (fw 6.4-8, European, Heat Link, MQTT).
- Keeps stock nlclient; **while Fledge cards are shown, nlclient hands the display and ring to nestlocal.** Useful reference for the handoff mechanism and ring handling. Docs describe no app SDK.

### Q-SYS QRC — https://help.qsys.com/Content/External_Control_APIs/QRC/QRC_Commands.htm  [verified unless noted]
- TCP **port 1710**, JSON-RPC 2.0, **every message NUL-terminated**. Works against a Core or Designer emulation.
- `Logon {User, Password}` only needed if the Core has users configured.
- `Component.GetComponents` → all **Named Components** (the docs example passes a dummy `"params":"test"`). `Component.GetControls {Name}` → controls of one component; fields seen in docs: `Name`, `Value`, `String`, `Position`, `Direction` (e.g. "Read/Write"). **[verified against Designer emulation, 2026-11-13]** every control has `Type` (seen: Boolean, Float, Integer, Text, Trigger, Status), `Direction` and `String`; Float/Integer add `ValueMin`/`ValueMax` and `StringMin`/`StringMax`; Booleans are JSON `true`/`false`; **Text has no `Value`, only `String`**; Trigger's `Value` is `0.0`. Script components have some controls with no `Type`. Raw responses: `crates/qsys/tests/data/designer/`.
- **Custom Controls' indicators (LED, meter, display text) report `Read/Write`**, and Toggle and Momentary buttons are both `Boolean`. Direction and Type alone cannot tell an indicator from an input, or a momentary from a toggle. **Decided:** in `custom_controls` components the names Q-SYS gives decide (`led.N` indicator, `meter.N` meter, `text.display.N` label, `momentary.N` momentary button); a script component's own controls (`code`, `reload`, `log.*`, `script.*`, ...) are hidden.
- Custom Controls' control names contain dots (`text.1`, `text.display.1`). The `NEST_` prefix is matched **in any case** (`Nest_Combo` counts).
- A component with Script Access None is not listed by `GetComponents` at all (`Nest_hidden` was in the design and absent).
- Custom Controls has no combo box; use a Text Controller (`tools/qsys/combo_box.lua`). Knobs are Gain components set to knob style.
- **A combo box's choices are not in `GetControls`, but `Component.Get` and every `ChangeGroup.Poll` change carry `Choices`** (a Text Controller combo, `combo_box.lua`). Discovery fetches them with one `Component.Get` per component for its Text controls; a Text control with choices becomes a picker.
- `Component.Get` and polls report **no `Type`**, **Booleans as `0.0`/`1.0`** (not `true`/`false` as in `GetControls`) and a Text control's `Value` as a meaningless `0.0` (its text is `String`). Polls also carry `Legend` (**verified 2026-11-13: a Text Controller's `Legend` set from its script reaches the Nest as the control's label**), `Disabled`, `Invisible`, `Color`, `CssClass`, `Indeterminate`; `Choices` is `[]` on Custom Controls and absent on gain controls.
- A Text Controller's single control is named `Control` (no index). Its script's own controls (`code`, `log.*`, `script.*`, ...) change often (log, memory), so they are left out of change groups.
- Momentary buttons report inconsistent edges over polling and are rarely used in designs: supported simply (true on press, false on release), not worth more time.
- Change groups: the first `Poll` reports every subscribed control; changes include `Component`. `Component.Set` works for Float and Boolean.
- `Control.Get/Set` operate on loose **Named Controls by known name**; **no method to enumerate them was found** (docs seen only as excerpts). Hence discovery via Named Components + Code Names.
- Component Code Name and **Script Access** (All / External / None / Script) are set in the component's properties. Use `NEST_` Code Names with Script Access External/All.
- `ChangeGroup.AddComponentControl` + `ChangeGroup.Poll` for change notification (max 4 change groups). Keepalive: a `NoOp` method exists to keep the socket open.
- A manifest alternative (a Lua script in the design publishing a JSON description into one string control) was discussed as a later enhancement for labels/ordering/ranges; not chosen for v1.

## Widget mapping (v1)

| Control | Widget |
|---|---|
| Boolean, writable | toggle (click flips) |
| Trigger | button (click fires) |
| Number with min/max, writable | dial (ring adjusts, click confirms/next) |
| Number, no range | number entry (ring steps value) |
| Text, writable | picker/preset list (ring text entry is awkward) |
| Any read-only | label / indicator / meter |

Reference implementation of this mapping: `pick_widget()` in `qrc_discover.py`.

## Test script (works now, no Nest hardware needed)

- Both live in `tools/qsys/`; `tools/qsys/README.md` has the Designer test procedure.
- `qrc_discover.py CORE_IP [--prefix NEST_] [--user U --password P] [--raw] [--watch SEC] [--all] [--set COMP.CTRL=VALUE]` — connects, logs on if asked, lists `NEST_*` components, prints each control with the widget it would get, optionally follows changes via a change group.
- `mock_qrc_server.py [port]` — fake Core for offline testing. **Its response shapes are assumptions** (Type/ValueMin/ValueMax included), so a passing mock test proves script logic only.
- Tested 2026-11-12 against the mock: discovery, prefix filtering, widget mapping, watch mode all worked.
- **First real task for Erik:** run `qrc_discover.py <core-or-emulator> --raw` against a Designer file with a few `NEST_` components (gain, mute, source select, trigger, read-only text) and confirm which fields the Core really returns. Fix `pick_widget()` / the mock to match.

## Found in this repository (2026-11-13)

This repository (`ecarlson88/Nest-Sys`) already is the unfr0zen fork; it has
only `origin` as a remote so far. Read in the code, **not yet seen on
hardware**:

- **Display:** `/dev/fb0`, 480x300 on the gen 3; Doom draws to it directly
  and pans/unblanks it (`platform/common/doom/doomgeneric_nest.c`).
- **Ring and click:** evdev devices under `/dev/input/`: the ring is
  `EV_REL`/`REL_DIAL`, the click `EV_KEY`; Doom takes them with `EVIOCGRAB`
  so nlclient sees nothing. A piezo takes `EV_SND` tones (click feedback).
- **Screen hand-over:** a patched nlclient shows nothing while the pid in
  `/var/run/unfr0zen/screen.owner` is alive, but keeps running
  (`platform/common/nlclient/README.md`, "Screen hand-over"). Otherwise the
  launcher stops nlclient through monit and restarts it after
  (`crates/nestlocal/rootfs/doom.sh`). nestlocal launches Doom when the
  UNFR0ZEN screen leaves a request file (`crates/nestlocal/src/doom.rs`).
  A Q-SYS app can start the same way.
- **New risk: suspend.** The unit suspends whenever nothing holds it awake
  (`nlsleepd`), and asleep wakes only for registered Wi-Fi packet patterns
  (`crates/nestlocal/src/wake.rs`). A control surface must hold the unit
  awake and keep its Core connection; this probably needs steady power.
  Replacing nlclient also means finding out what it does for Wi-Fi and sleep.
- **Toolchain:** device code is Rust for `armv7-unknown-linux-musleabihf`,
  built with cargo-zigbuild (`utils/build-firmware.sh`).

## QRC client crate: `crates/qsys`

The device-side QRC client, std + serde_json, blocking:
- `Client`: framing, `Logon`, `Component.GetComponents/GetControls/Get/Set`,
  `ChangeGroup.AddComponentControl/Poll`, `NoOp`; unsolicited messages kept.
- `Transport` trait (`list`/`get`/`set`/`subscribe`/`poll`) with a QRC
  implementation, so another transport can stand in later.
- `widget::pick`: the v1 widget mapping, same as `pick_widget()`.
- Tests against an in-process fake Core (`crates/qsys/tests/`); the example
  `cargo run -p qsys --example discover -- HOST` runs against Designer.

## The screen: `crates/surface`

One screen per component (decided 2026-11-13). A Gain component is a volume knob
(ring = level, click = mute, bypass and invert hidden). Invisible or
Disabled controls and bypassed components are not shown (decided
2026-11-13). `app` is the state machine
(ring, click, hold; edit mode; component list), `render` draws the 480x480
round face, `canvas::to_panel` lays it out turned and packed for the gen 3's
480x640 panel, and `input` turns `REL_DIAL` counts (120 a detent, negative
clockwise) and `KEY_ENTER` into turns, clicks and holds. The `sim` binary
runs it against a Core in a browser (`tools/qsys/README.md`). The simulator
reconnects when the Core goes away and rediscovers the design. Still to do
for the unit: a framebuffer and evdev backend in place of the browser.
Text is Inter (`crates/surface/fonts`, OFL) drawn with fontdue; shapes are
anti-aliased.

Settings (the Controls list's last entry): helper text on/off and a
Network page; to add on hardware: screen timeout, brightness, and Wi-Fi
details (SSID, signal) on the Network page.

Home screen: the time and date after the idle delay (Settings); on the
unit, dim the backlight there too. The Controls list and combo box choices roll
as a drum (render::Renderer keeps the drum's place between frames); on the
unit, check the frame rate the CPU manages for full-face redraws.

## How nlclient works (from this repository; not yet seen on hardware)

nlclient (`/nestlabs/sbin/nlclient`) is Nest's own program on the unit: the
thermostat UI, the regulation of the heating through the backplate, the
client of Nest's cloud and the owner of much of the unit's housekeeping.
What the code and docs here say about it:

- **Cloud client.** It talks to Nest's service for everything, keeping its
  state in the cloud's "buckets" (device, shared, structure, schedule...). It
  holds a long-poll subscribe open almost all the time (`hold`, 290 s).
  unfr0zen points its `cloudregisterurl` at nestlocal, which serves those
  endpoints on the unit itself (`crates/nestlocal/src/cloud.rs`,
  `liveness.rs`). "nlclient keeps the truth; the server keeps a copy."
- **Clock.** It sets the unit's clock from the cloud's
  `X-nl-service-timestamp`. A unit with no ntpd boots in 2000 and nothing else
  sets it, so nestlocal sets it by NTP or from Home Assistant
  (`crates/nestlocal/src/clock.rs`). **For us:** without nestlocal or the
  cloud, the home screen's time would be wrong unless the surface (or the
  system) sets the clock by NTP.
- **Sleep.** The unit suspends whenever nothing holds it awake: `nlsleepd`
  ("nlscpm" on D-Bus, `com.nestlabs.scpm.sleepd`). Asleep, it wakes only for
  packets the Wi-Fi chip was told to watch, which nlclient's own "sniffer"
  programs for its cloud connection. A program can be a sleep entity
  (`RegisterEntity`; nlsleepd calls `RequestSleep` before each sleep and
  `WakeUp` after) and decline sleep, as nestlocal does while it needs to
  (`crates/nestlocal/src/wake.rs`). **For us:** the surface's QRC connection
  will not wake a sleeping unit; it likely has to be a sleep entity that
  declines sleep while the screen is on (or keep the unit awake for good on
  mains power).
- **Display and activity.** nlclient lights and dims the display by user
  activity; "stock user-activity notifications keep the display and device
  awake" (`platform/common/nlclient/README.md`).
- **Screen hand-over** (patched nlclient only, same README): while the pid in
  `/var/run/unfr0zen/screen.owner` is alive, nlclient keeps running (and
  regulating) but shows nothing. Gen 3 from 6.x draws with **EGL** (kind 1:
  its `eglSwapBuffers` is skipped); the 2019 gen 3 and the gen 2 draw
  straight into fb0 (kind 2: its mapping is swapped for private memory).
  Meanwhile a thread in nlclient "lights the display every 2 s", since the
  owner grabs the ring and button and nlclient would otherwise see no
  activity. **nlclient writes `keeper.pid` only at its first bucket commit
  or a click on the UNFR0ZEN screen**; until then a launcher stops nlclient
  instead of handing over. **For us:** at boot, the launcher may find no
  `keeper.pid` yet and fall back to stopping nlclient.
- **Starting programs.** nlclient cannot start a program; the UNFR0ZEN
  screen (code injected into nlclient, `platform/common/nlclient/src`)
  writes a request file in `/var/run/unfr0zen` (`doom.request`) and
  nestlocal launches it (`crates/nestlocal/src/doom.rs`). A "Q-SYS" entry
  there could start the surface the same way.
- **Startup order.** nestlocal starts before nlclient: nlclient opens the BLE
  chip's UART and reads its settings file as it starts, and nestlocal must
  have its stand-ins in place first (`crates/nestlocal/rootfs/nestlocal.init`,
  `sensor/port.rs`, `structure.rs`).
- **Patching is reversible.** The patched nlclient keeps the original at
  `/etc/unfr0zen/nlclient.original` and an undo journal
  (`/etc/unfr0zen/nlclient.undo.gz`).
- **Bench use.** unfr0zen's `--backplate-sim` gives a "1H1C with a simulated
  temperature, for testing off the wall" (`docs/integrations/index.html`):
  probably what a unit on the bench, without heating wiring, needs.
- **Not covered here:** how Wi-Fi is configured and kept up (nlclient's
  setup screens, presumably), and whether anything watchdogs nlclient or
  the unit. Find out on hardware (`ps`, `/etc/init.d`, `/etc/monit*`).

## Hardware decision: keep the stock stack, or go "nuclear"

Once the surface runs on a unit, judge the stock route (unfr0zen + the OEM
nlclient + our app beside it) by these:

**Signs the stock stack is fine** (then: an upstream PR to unfr0zen for a
generic "run this app instead of the thermostat UI" option, and a repo of
our own holding only the Q-SYS Rust app):
- the hand-over is clean: no flicker, nlclient never takes the screen back;
- the unit stays awake and on Wi-Fi while the surface runs, without fights;
- it comes back by itself after a reboot or a Wi-Fi drop;
- frames draw comfortably fast with nlclient running behind.

**Signs to go nuclear:**
- fighting `nlsleepd` to stay awake;
- display glitches, or nlclient grabbing the screen back;
- nlclient's CPU or memory use making the animation stutter;
- a slow or fragile start that waits on the stock software.

**Nuclear** means replacing the stock userland with a minimal system: our app
plus only what brings up Wi-Fi, keeps the time, and drives the display and
ring, perhaps with our own kernel and root file system. unfr0zen's recovery
and flashing still matter (that part is worth tracking upstream); less of
what it installs does. More work (Wi-Fi, power, any watchdog become ours),
but a lean, fast-booting device that is wholly ours.

Either way the Q-SYS app changes little: its front end (`device.rs`) is
separate from everything else (`runner.rs`), which is why it should end up
in a repository of its own whichever route wins. Repository options talked
through (2026-11-13), none acted on: keep this whole fork; a fork with the
unwanted parts deleted (constant conflicts; avoid); or two repositories, an
almost-stock unfr0zen fork plus a separate Q-SYS app repository (preferred,
after hardware).

## Suggested milestones

1. Real-Core run of `qrc_discover.py --raw`; settle the field names and widget rules.
2. Fork unfr0zen; build unmodified; confirm output matches the upstream release.
3. Get a used Gen 3. **Take a NAND backup before anything else**, then root with the stock tooling; confirm recovery works.
4. SSH in (`--ssh`). Identify display (framebuffer?) and ring/click (input event devices?) nodes. Read the doomgeneric port source and the `nlclient`/`nestlocal` handoff (see Fledge) to learn how apps take over the screen/ring.
5. "Hello world" app that takes the screen through the hand-over (as Doom
   does), with nlclient still running; boot-in-place-of comes in step 7.
6. Put `crates/qsys` on the device behind a framebuffer UI (Rust, built like
   nestlocal).
7. Boot-to-app image: remove/disable stock thermostat software cleanly; keep the recovery path intact.

## Open questions / risks

- **[unknown]** Does the stock software feed a watchdog or talk to the heating hardware, such that stopping it reboots the unit? Check in milestone 4.
- **[unknown]** Bench power: how to power a unit without furnace wiring (USB alone may not suffice). Check the unfr0zen Discord/issues before assuming.
- **[unknown]** Display/ring interfaces and whether sockets/libraries are available in the on-device userland.
- Keep upstream as a git remote; keep Q-SYS code in its own module/crate to ease rebasing. Don't modify the flashing/recovery code unless necessary — recovery is what prevents bricks.
- Gen 3 only; Thermostat E and Gen 4 are not listed as supported. Ask sellers for the firmware version and photos of the unit/backplate.
- Rooting is "as is" with no warranty and voids the Nest warranty. Use a spare unit, not the one heating the house.
- Fork must remain GPL-3.0 if distributed.

## Sources
- unfr0zen: https://unfr0zen.com/ · https://github.com/unfr0zen/unfr0zen · BUILDING.md in that repo
- Fledge: https://github.com/sysmoon14/fledge
- QRC: https://help.qsys.com/Content/External_Control_APIs/QRC/QRC_Commands.htm
- Q-SYS Lua Component API: https://help.qsys.com/q-sys_9.5/Content/Control_Scripting/Using_Lua_in_Q-Sys/Component.htm
