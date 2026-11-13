# Taking over: the Nest as a Q-SYS control surface

Notes for the next Claude Code session (written 2026-11-13, at the end of the
first one). Read this first, then `HANDOFF.md` (background, facts found,
decisions, how nlclient works) and `README.md` (how to use the tools, the
simulator and the unit build; **the reference for how the screen behaves
today**). Where an older part of `HANDOFF.md` disagrees with `README.md` or
the code (its early "Widget mapping (v1)" table, say), the latter are current.

## Where things stand

- Branch **`ccr-deb37764-czgzbi`** of `ecarlson88/Nest-Sys` (a fork of
  `unfr0zen/unfr0zen`). 31 commits on top of the fork's `main` (`8e095b3`), all
  pushed. No `upstream` remote is set up yet.
  **Not merged, no pull request: Erik does not want either yet.** Do not open
  one or touch `main` unless asked.
- No hardware yet; Erik expects a used Nest Gen 3 within days. Everything so
  far was tested against **Q-SYS Designer's emulation** (by Erik, on Windows)
  and against mocks here. Nothing has run on a Nest.
- Tests: `cargo test -p qsys -p surface` (79 tests) and
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
| `crates/surface/{build-unit.sh,install-unit.sh,rootfs/}` | Cross-build for ARMv7, copy over SSH to `/media/data/nest-surface`, launcher using nlclient's screen hand-over (as Doom's), init script, monit file. |
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
- **No force pushes.** History is left as is: commit `7cc6d86` does not build
  (`nest-surface.rs`); `7d16391` fixes it. Every commit after builds.

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

1. **`nest-surface --diag`**: print the framebuffer (size, bits a pixel,
   channel offsets, pages, stride) and the input devices; draw an orientation
   pattern (an up arrow, colour bars, the face's round edge) through the real
   panel path; show and log each ring detent (with direction) and click.
   First thing to run on the unit.
2. **Connect off the loop**: `Runner::try_connect` blocks up to 3 s
   (`Client::connect` timeout) and discovery more; on the unit that freezes
   the ring. Move connecting to a thread and hand the session back; keep
   drawing "Connecting..." meanwhile.
3. **Runner tests** against the Rust mock (`crates/qsys/tests/mock`, which
   would need to become shareable, e.g. a `mock` feature or a test-support
   module): connect, Core gone, reconnect to the same screen, settings saved.
4. **Hardware session checklist** at the top of `HANDOFF.md`: Windows + WSL2
   Ubuntu for builds and SSH, NAND backup first, ECDSA key and legacy ssh
   options for the gen 3's OpenSSH 6.0, `--backplate-sim` for the bench, then
   the first commands (`--diag`).
5. Optional: keep-awake as nestlocal does (`crates/nestlocal/src/wake.rs`:
   D-Bus `RegisterEntity` with nlsleepd, declining `RequestSleep`), behind an
   off-by-default option.

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
4. `--diag`, then `install-unit.sh` and `nest-surface.sh`: face upright and
   round, ring direction, frame rate, the hand-over (note `keeper.pid` is
   written only after nlclient's first bucket commit or a click on its
   UNFR0ZEN screen), and whether the unit suspends.
5. Then Settings that need hardware, starting at boot, and the stock vs
   nuclear decision.

A Claude Code session on Erik's own PC (or `claude remote-control` there) can
reach the unit over SSH, Designer and the backup; a cloud session cannot.
