# Taking over: the Nest as a Q-SYS control surface

Notes for the next Claude Code session (written 2026-11-13, at the end of the
first one). Read this first, then `HANDOFF.md` (background, facts found,
decisions, how nlclient works) and `README.md` (how to use the tools, the
simulator and the unit build; **the reference for how the screen behaves
today**). Where an older part of `HANDOFF.md` disagrees with `README.md` or
the code (its early "Widget mapping (v1)" table, say), the latter are current.

## Where things stand

- **All work goes on branch `feature/qsys`** of `ecarlson88/Nest-Sys` (a
  fork of `unfr0zen/unfr0zen`), on top of the fork's `main`; Erik wants no
  other session branches (2026-11-15). The earlier session branches were
  folded into it. No `upstream` remote is set up yet.
  **Not merged, no pull request: Erik does not want either yet.** Do not open
  one or touch `main` unless asked.
- Hardware since 2026-11-14: the surface runs on Erik's gen 3 (also his
  house thermostat; see HANDOFF.md, "Erik's unit as patched"), against
  Designer's emulation; a sacrificial second unit is coming.
- Tests: `cargo test -p qsys -p surface` (86 tests) and
  `cargo clippy -p qsys -p surface --locked --all-targets -- -D warnings`,
  both clean. They also pass as 32-bit ARM under `qemu-arm` (below).

## What is in the branch (all new; upstream files untouched except
`Cargo.toml`/`Cargo.lock` gaining the two crates and `.gitignore` one line)

| Path | What |
|---|---|
| `crates/qsys` | QRC client (std + serde_json, blocking): framing, Logon, GetComponents/GetControls/Get/Set, change groups, NoOp; `Transport` trait; widget choice (`widget::pick_for`). Fixtures from Designer in `tests/data/designer/`. |
| `crates/surface/src/app.rs` | The screen's state machine, pure and tested: screens per component, layouts (Volume, Selector, Controls), edit mode, Controls list, Settings, idle home screen, hiding (Invisible/Disabled/bypass), Mute folded into a Level. |
| `crates/surface/src/render.rs` | Draws the round 480x480 face; drum-style wheels animated by `Renderer`. |
| `crates/surface/src/canvas.rs` | Anti-aliased shapes, Inter text (fontdue), round mask, PNG, and the gen 3 panel layout (480x640, turned). `fonts/`: Inter 300/400/600, OFL. |
| `crates/surface/src/runner.rs` | Shared loop for every front end: connect/retry/reconnect, inputs, polling, clock, settings file, drawing. |
| `crates/surface/src/input.rs` | Ring counts (120 a detent, negative clockwise) to detents; click/hold (700 ms). |
| `crates/surface/src/device.rs` | Linux only: `/dev/fb0` (two pages, pan), console to graphics, `/dev/input` grabbed (`REL_DIAL`, `KEY_ENTER`), restore on exit. **Written from the Doom port, never run on a unit.** |
| `crates/surface/src/bin/sim.rs` + `sim.html` | Simulator: serves the face to a browser (mouse, keyboard, phone touch). |
| `crates/surface/src/bin/nest-surface.rs` | The unit's program: config from `/media/data/nest-surface/config.json`. |
| `crates/surface/{build-unit.sh,install-unit.sh,rootfs/}` | Cross-build for ARMv7, copy over SSH (the program to `/nestlabs/sbin`, the rest to `/media/data/nest-surface`), launcher using nlclient's screen hand-over (as Doom's), init script, monit file. |
| `crates/surface/examples/bench.rs` | Frame timings (desktop: ~0.2-0.5 ms a screen, ~1 ms to lay out for the panel). |
| `tools/qsys/` | `qrc_discover.py` (+ `--raw`, `--get`, `--set`, `--watch`), `mock_qrc_server.py`, `test_bed.lua` (Text Controller with one of each control), `combo_box.lua`, `HANDOFF.md`, `README.md`. |

## Decisions Erik made (keep to them)

- One screen per component. Code Name prefix `NEST_`, any case. Script Access
  None and `bypass` on keep a component off the Nest.
- Custom Controls' widgets come from Q-SYS's own names (`led.N`, `meter.N`,
  `momentary.N`, `text.display.N`); a script component's own controls
  (`code`, `log.*`, `script.*`, ...) are hidden.
- Gain component = volume knob: ring sets level at once, shown 0-100% (1% a
  detent), click mutes, red when muted, a turn while muted only unmutes.
- A toggle named **Mute** beside a dial (the one named **Level** if several)
  folds into it: same look; click to adjust, then ring = level, click =
  mute, hold or 5 s = done.
- A component whose only control is a combo box: the ring scrolls the
  choices at once, a click selects (nothing sent before), 5 s reverts.
  Choices come from `Component.Get` (GetControls lacks them).
- Editing ends after 5 s without input. "Controls" (not "Components") names
  the list; it ends with Settings; hold does nothing inside it. Each screen
  ends with a Controls stop. Hold on a momentary keeps it pressed.
- Labels: the design's Legend (verified to reach the Nest) else a readable
  name ("toggle.1" -> "Toggle").
- Settings: helper text on/off, home screen delay (30 s/1 min/5 min/never),
  12/24-hour clock, Network page. Later, with hardware: screen timeout,
  brightness, Wi-Fi details.
- Momentary buttons are rough over polling and rarely used: low priority.
- Repository plan, after hardware (nothing done): likely an almost-stock
  unfr0zen fork plus a separate repo for the Q-SYS app, and a PR to upstream
  for a generic "run this app instead of the thermostat UI" option. If the
  stock stack proves clunky, a "nuclear" minimal system instead. Criteria in
  `HANDOFF.md`, "Hardware decision".

## How Erik likes to work

- Asks for clarifying questions when something is ambiguous; prefers "I
  don't know" to guesses. Offer options with a recommendation, then build.
- Tests in Designer on Windows (PowerShell; `py`), pastes output back.
- Wants commits pushed as work lands, in the repo's style (`AGENTS.md`):
  `subsys: lowercase summary`, `qsys:`/`surface:`/`docs:` prefixes used
  here, one logical change per commit, staged by explicit path.
- **No force pushes** from a session. (Erik rebased `feature/qsys` onto a
  newer `main` himself; that is his call.)

## Tooling notes

- Device build: `sh crates/surface/build-unit.sh` needs zig 0.16.0 and
  cargo-zigbuild 0.23.4 (`BUILDING.md`). In the cloud container: zig via
  `pip install ziglang==0.16.0` in a venv put on `PATH`;
  `cargo install --locked cargo-zigbuild@0.23.4`. Output:
  `build/nest-surface-armv7` (~930 KB, static).
- ARM tests: `cargo zigbuild --tests -p surface -p qsys --target
  armv7-unknown-linux-musleabihf --message-format=json`, then run each test
  executable with `qemu-arm` (`apt-get install qemu-user`), from
  `crates/surface` so fixture paths resolve.
- Simulator: `cargo run --release -p surface --bin sim -- CORE_HOST`, open
  http://127.0.0.1:8480 (`--listen 0.0.0.0:8480` for a phone). Offline:
  `python3 tools/qsys/mock_qrc_server.py 11710` and `--port 11710`.
- Screenshots in a session: Playwright + Chromium are preinstalled.
- GitHub raw downloads are blocked from the cloud container; package
  registries (crates.io, npm, PyPI, apt) work.

## Next, before hardware (agreed list, not started)

1. ~~**`nest-surface --diag`**~~: done (`src/diag.rs`, README.md "On the
   unit"). First thing to run on the unit.
2. ~~**Connect off the loop**~~: done; connecting and discovery run on a
   thread. Still blocking: a Core that is connected but stops answering
   holds a step for up to 3 s per request (`exchange`) before it counts as
   lost. Rare; worth doing only if seen.
3. ~~**Runner tests**~~: done, `crates/surface/tests/runner.rs`, sharing
   qsys's mock by `#[path]`.
4. ~~**Hardware session checklist**~~: done, at the top of `HANDOFF.md`.
5. Optional: keep-awake as nestlocal does (`crates/nestlocal/src/wake.rs`:
   D-Bus `RegisterEntity` with nlsleepd, declining `RequestSleep`), behind an
   off-by-default option.

## Status, 2026-11-15

The surface runs on Erik's gen 3 against Designer's emulation (HANDOFF.md,
"Erik's unit as patched"). Open from that first run: how smooth it feels
(~30 frames a second; the panel layout step is the slow part), whether
nlclient ever takes the screen back, whether the unit sleeps while the
surface runs, and starting at boot.

## Apps in unfr0zen (Erik's decision, 2026-11-15; not started)

The upstream feature should be generic **Apps**, not a Q-SYS entry: an
Apps section in the unfr0zen exe where users tick the apps to install, as
Doom is ticked today, and an Apps entry on the UNFR0ZEN screen to launch
one. Doom's wiring is the model (firmware build -> host assets ->
`patch.rs` `Options::doom` overlay and nestlocal's `-doom` flag -> the
UNFR0ZEN screen's page in nlclient, `platform/common/nlclient` -> a
`.request` -> `crates/nestlocal/src/doom.rs` runs the launcher with the
screen hand-over). For the Apps feature, later: the hand-over's keeper thread
(`custom_screen.c`, `keeper` -> `wake_display` -> `light_display`) both
marks activity (keeps nlclient from sleeping) and returns
`nlUIResultLightDisplay` (relights at nlclient's brightness) every 2 s, so
an app cannot own the backlight while nlclient runs. Idea: while the
screen is owned, mark activity but return a result that does not light
the display, so the app owns the backlight. Needs the patched nlclient
rebuilt, its tests (`platform/common/nlclient/tests`) and a unit to try.
Erik decided: apps come as **bundle files**
(the Q-SYS app can live in its own repo); an app's settings (Q-SYS's Core
address) are entered **on the Nest itself**, with a ring keyboard like
the stock one (HANDOFF.md describes it); raise the idea upstream first: a
draft issue is in `UPSTREAM_APPS_ISSUE.md`, **for Erik to post, not us**.

## Later: an accent colour set from Q-SYS (Erik's idea, 2026-11-15)

To brand a unit to a customer's colour, the accent blue should be settable
from the design, by a dropdown rather than a colour picker. Leads, none
checked: polls already carry a `Color` field per control (HANDOFF.md, QRC
facts), never inspected; Erik suggests a Call Sync component's User Button
colour dropdown. Call Sync (for syncing mic LEDs and panel accents to call
status; controls listed by Erik in Designer) has, per state (idle, mute,
unmute, ringing) and for the user button, a `*.color` String (the
dropdown) and a `*.color.rgb` String twin, e.g. `user.button.color` and
`user.button.color.rgb`; not yet read: whether `.rgb` follows the dropdown
and its format (`qrc_discover.py IP --get Sync.user.button.color.rgb`).
Open choice: borrow the room's Call Sync colour, or a `NEST_` setting of
its own; a fallback is a `NEST_` combo box choosing from a fixed
list of colours. Settled UI choices to keep: no on/off or true/false text
on toggles or indicators; buttons keep "Press"/"Hold", Settings keeps
"Helper text: On/Off".

## Later: the Nest's sensors as Q-SYS triggers (Erik's wish)

Erik wants the unit's sensors as triggers for Q-SYS. Found so far
(HANDOFF.md): nlclient follows an ambient light sensor (auto-brightness)
and a far-field presence sensor (display on when someone is in view at
4-5 ft, off after they leave). Neither is a Linux device; nlclient holds
one serial port, `/dev/ttymxc5`, the best lead. Next: nlclient's log for
what it calls them; then, on the sacrificial unit with `--exclusive`
(nlclient stopped), listen on `ttymxc5`. If readable: publish presence and
light to the Core (e.g. as controls of a NEST_ component) and use presence
to wake the surface.

2026-11-20: on USB power alone `ttymxc5` read nothing in 30 s with
nlclient stopped (HANDOFF.md). Next: repeat on a powered base (24 V AC).
2026-11-21: on a powered base it sends framed messages unasked
(format and checksum in HANDOFF.md). Next: timed captures (still, covered,
lit, approach) to tie message types to light and presence.
2026-11-21, later: nlclient traced with strace while running (HANDOFF.md):
light found (`0x0a`), the host's frames and a set-up burst that wakes the
PIR data. Next: a presence trace with Erik in the room; then try replaying
the set-up with nlclient stopped (and the unit held awake).

## Later: power from USB alone (Erik's wish, 2026-11-14)

Erik wants the unit to run from its USB port only, without the base's 24 V
(C wire). On USB alone the stock UI shows only "reconnect to base", but
unfr0zen's own screens showed during flashing, so the display and SoC run
on USB. Likely routes, not yet tried: the backplate simulator
(`--backplate-sim`, not in the current patch), and our surface owning the
screen anyway through the hand-over. Unknown: whether nlclient or nlsleepd
shut down or sleep without a base, and how the battery charges on USB.

## With hardware (in order)

1. **NAND backup before anything writes to the unit**; keep it outside the
   repository (never commit dumps or credentials, `AGENTS.md`). Ideally from a
   unit not yet on Erik's Wi-Fi or Nest account.
2. Root with unfr0zen (the Windows app's window; its CLI is not on Windows),
   with SSH. Recovery path: confirm it works.
3. Look around over SSH: `ps`, `/etc/init.d`, `/etc/monit*`, D-Bus services,
   who manages Wi-Fi, any watchdog, `/sys/class/backlight`, `/dev/fb0` and
   `/dev/input`. A NAND backup can also be unpacked offline (UBIFS root) to read
   the same; findings, in our own words, go in `HANDOFF.md`.
4. `install-unit.sh`, `nest-surface.sh --diag`, then `nest-surface.sh`:
   face upright and round, ring direction, frame rate, the hand-over (note `keeper.pid` is
   written only after nlclient's first bucket commit or a click on its
   UNFR0ZEN screen), and whether the unit suspends.
5. Then Settings that need hardware, starting at boot, and the stock vs
   nuclear decision.

A Claude Code session on Erik's own PC (or `claude remote-control` there) can
reach the unit over SSH, Designer and the backup; a cloud session cannot.
