# Nest Gen 3 as a Q-SYS control surface — handoff

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
- **Custom Controls' indicators (LED, meter, display text) report `Read/Write`**, and Toggle and Momentary buttons are both `Boolean`. Direction and Type alone cannot tell an indicator from an input, or a momentary from a toggle. **[open decision]**
- Custom Controls' control names contain dots (`text.1`, `text.display.1`). Prefix matching is case-sensitive (`Nest_Combo` is not matched by `NEST_`).
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
