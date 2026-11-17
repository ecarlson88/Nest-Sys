# OpenAVC: findings and handoff

Notes for whoever builds the OpenAVC control surface. Nothing here has been
built, run or tested: every statement about OpenAVC comes from reading its
source at `open-avc/openavc` `9c48255` (2026-11-16, version 0.40.0), and
file references below are to that commit. Statements marked **[unverified]**
are inferences to check against a running server. Facts about the unit come
from [`tools/qsys/HANDOFF.md`](../qsys/HANDOFF.md), where they were measured.

## What OpenAVC is

- A Python server (`requires-python >= 3.11`): FastAPI and uvicorn for HTTP
  and WebSockets, pydantic for its project model, pyserial, httpx, gmqtt,
  croniter, psutil, aiohttp and cryptography (`pyproject.toml`).
- A project (`.avc`, JSON) holds devices (drivers in YAML or Python),
  variables, macros, triggers, scripts and the touch-panel UI.
- Two web front ends: the **Programmer** IDE (`/programmer`) and the
  **Panel** (`/panel`), plain JavaScript (`openavc/web/panel/modules/`).
  On its own hardware it shows the panel in Chromium kiosk mode
  (`docs/deployment.md`).
- Panels are admitted by approval: a new tablet shows a six-digit code
  until someone approves it in the Programmer (`openavc/api/panel_access.py`).

## The Nest as an OpenAVC panel

The Nest connects to an OpenAVC server as a panel, like a tablet, and shows
its controls on the round screen with the ring and click, as `nest-surface`
does for a Q-SYS Core.

### Getting admitted

`GET /api/panel/access` (`panel_access.py:231`) is the check-in a panel
makes first. It answers:

- `{"access": "open"}` when **Panel access** is set to anyone on the
  network;
- otherwise `{"access": "approved", "status": ...}`, with `"code"` (the
  six digits) and `"space"` (the space's name) while the device is
  pending, or `"name"` once approved.

A first check-in creates a pending record and sets a pending cookie (one
day). Once approved in the Programmer, the next check-in collects the real
cookie, `openavc_panel_<instance id>` = `<device id>.<secret>`, which lasts
400 days (`COOKIE_MAX_AGE_SECONDS = 34_560_000`, `core/panel_devices.py:95`)
and is re-issued when a day old. A waiting panel polls every 3 seconds.
`POST /api/panel/access/claim` with the admin password as HTTP Basic
approves the device from the panel itself.

On the Nest: show the code and the space's name, poll every 3 s, and keep
the cookie in the program's settings file (mode 0600; it is a credential).
A connection closed with **4010** (`CLOSE_CODE_NOT_APPROVED`) means the
device is not, or no longer, approved: go back to the check-in.

### The WebSocket

`ws://HOST:8080/ws?client=panel` (`openavc/api/ws.py:111`), with the panel
cookie. 8080 is the default HTTP port; HTTPS is 8443 when turned on.

On connect the server sends, in order:

1. `{"type": "state.snapshot", "state": {KEY: VALUE, ...}}`
2. `{"type": "ui.definition", "ui": {...}}` (when a project is loaded)

After that, changes arrive as `{"type": "state.update", "changes":
{KEY: VALUE}}` and `{"type": "state.delete", "keys": [...]}`
(`core/ws_hub.py:316`). State is pushed, not polled: unlike QRC there is
no 200 ms poll. `?namespaces=var,device` limits state to those key
prefixes. The server sends `{"type": "ping"}` every 30 s; answer
`{"type": "pong"}`.

What a panel may send (`_PANEL_ALLOWED_TYPES`, `ws.py:671`):

| Message | Fields | Use |
|---|---|---|
| `ui.press`, `ui.release`, `ui.hold` | `element_id` | buttons |
| `ui.toggle_off` | `element_id` | a toggle-mode button turned off |
| `ui.change` | `element_id`, `value` | sliders, faders, selects |
| `ui.select` | `element_id`, `value` | a list row |
| `ui.submit` | `element_id`, `value` | keypad, text input |
| `ui.route` | `element_id`, `input`, `output`, (`audio`, `mute`) | matrix |
| `ui.page` | `page_id` | page change (answered with `ui.navigate`) |
| `macro.execute`, `macro.cancel`, `command`, `state.set`, `event.*` | | direct actions |

`value` must be a flat primitive. Limits: 200 messages a second per client,
100 connections per server.

The server runs each control's actions itself from its `do` bindings, so
the panel only reports what was touched. The Nest never needs to know
which device or macro a control drives.

### The UI definition

`ui.definition` carries the project's UI as `engine.panel_ui()` returns it
(`core/engine.py:293`): the stored model with matrices expanded, each
element stamped with the devices its actions reach, and the lock PIN left
out (`lock_enabled` instead). The model is in
`core/project_loader.py`:

- **Pages** (`UIPage`, line 708): `id`, `name`, `page_type`,
  `render_mode` (`elements`, or `custom` for an integrator's own HTML),
  `elements`, `layouts` (positions live in layouts, not elements).
- **Elements** (`UIElement`, line 521): `id`, `type`, `label`, `text`,
  `min`, `max`, `step`, `output_min`, `output_max`, `unit`,
  `display_decimals`, `options`, `items`, `target_page`, `parent` (the
  container holding it), `tag`, `style`, `bindings`, and type-specific
  fields.
- **Bindings**: `{"show": {...}, "do": {...}}`. `show` has `value` (the
  state key the control reflects), `look` (appearance from state),
  `visible`, and `items` for lists. `do` lists actions per interaction
  (`press`, `change`, `select`, ...). From
  `templates/simple_projector.avc`:

  ```json
  {"show": {"look": {"key": "var.system_power", "condition": {"equals": "on"},
                     "style_active": {"bg_color": "#4CAF50"}}},
   "do": {"press": [{"action": "macro", "macro": "system_on"}]}}
  ```

  A status LED's look is a map: `{"key": "var.system_power", "map": {"on":
  "#4CAF50", "off": "#F44336"}, "default": "#9E9E9E"}`.

The 18 element types (`docs/ui-builder.md`, "Element Types"): Button,
Label, Slider, Select, Status LED, Page Nav, Text Input, Image, Camera
Preset, Gauge, Level Meter, Fader, Container, Clock, Keypad, List, Matrix,
Custom Control. Plugins add more (`plugin_type`). Confirmed `type` strings:
`button`, `label`, `slider`, `status_led`, `page_nav`, `fader`, `matrix`;
the others are **[unverified]**.

### Mapping onto the surface

The surface's widgets are `qsys::Widget` (`crates/qsys/src/widget.rs`).
A first mapping, one OpenAVC page per screen as one `NEST_` component is
one now:

| OpenAVC element | Surface widget | Sends |
|---|---|---|
| Slider, Fader | `Dial { min, max }`, value from `show.value` | `ui.change` |
| Button | `Momentary` while held, or `Button` | `ui.press` on down, `ui.release` on up |
| Button in toggle mode | `Toggle`, lit from `show.look` | `ui.press` / `ui.toggle_off` **[unverified]** |
| Select | `Picker`, choices from `options` | `ui.change` |
| List | `Picker`, choices from `items` or `show.items` | `ui.select` |
| Status LED | `Indicator`, colour from `show.look.map` | |
| Label | `Label`, text from `text` or `show.value` | |
| Gauge, Level Meter | `Meter { min, max }` | |
| Keypad | `NumberEntry` | `ui.submit` |
| Page Nav | Not shown: the Controls list already moves between pages | |
| Container | Not shown: its children are flattened into the page | |
| Text Input, Image, Camera Preset, Clock, Matrix, Custom, plugins | `Hidden` | |

Range and taper to settle against a server: whether `output_min`,
`output_max`, `response` (logarithmic taper) and `send_throttle_ms` are
applied by the panel or the server **[unverified]**. If by the panel, the
Nest must apply them too. Elements whose `show.visible` is false, and
pages with `render_mode: custom`, are left out.

### The seam in the surface

`crates/surface` is tied to Q-SYS in two places:

- `app.rs` builds its screens from `qsys::{pick_for, Change, Component,
  Control, Discovered, Kind, Widget}`.
- `runner.rs` drives `qsys::{Client, Qrc, Transport}` on its own thread,
  polling every 200 ms (`POLL_EVERY`), and its `Config` is QRC's (host,
  port 1710, user, password, `NEST_` prefix).

Proposal: move the model types (`Component`, `Control`, `Kind`, `Widget`,
`Change`, `Discovered`) out of `qsys` into the surface (or a small shared
crate), and give the runner a backend interface (connect, discover, set a
control, wait for changes with a timeout, keep alive) that `qsys` and
`openavc` each implement. QRC polls inside its implementation; OpenAVC
waits on the socket. The Q-SYS behaviour and its tests stay as they are.

Each program then has its own binary, config and launcher:
`nest-surface` for Q-SYS as now, and `nest-openavc` with
`/media/data/nest-openavc/config.json` (server address, port, HTTPS, which
pages) and the same screen hand-over (`rootfs/nest-surface.sh`).

### On the unit

- The surface runs on the gen 3 only (`device.rs`: `mxs-lcdif`
  framebuffer, the ring's input device).
- A WebSocket client is needed in Rust; plain `ws://` keeps it small.
  HTTPS (`wss://` on 8443) would add a TLS library to the binary.
- Sleep is as for the Q-SYS surface: while it runs through nlclient's
  hand-over the unit likely cannot sleep (tools/qsys/HANDOFF.md, "Sleep
  while the surface runs"). OpenAVC pushes state, so the connection must
  stay up for the screen to stay current; on wake, reconnect and take a
  fresh snapshot.

## Open questions

1. **Which pages the Nest shows**: every page, pages whose ID starts with
   a prefix (as `NEST_` for Q-SYS), or pages named in the config. Not
   decided.
2. The `type` strings of the unconfirmed element types, and the exact
   shape of `show.value`, `show.items` and a select's `options`.
3. How a toggle-mode button reports its state and is turned off.
4. Whether output range scaling and taper are the panel's job.
5. Whether text input and keypads are worth entering with the ring.
6. Whether HTTPS is needed on the networks this will run on.

## First session with a server

No OpenAVC server is available yet. When one is, the way `tools/qsys`
started against Designer:

1. Run it from source on a PC (`pip install -e .`, `openavc`), or with
   its Docker image on Linux.
2. Build a test project with one page holding one of each element type,
   each bound to a variable, and set **Panel access** to approved panels.
3. Capture a panel's traffic: the check-in replies, `ui.definition`,
   `state.snapshot` and the `state.update`s while moving controls in the
   Programmer's preview. Keep it as test data, as
   `crates/qsys/tests/data/designer` does for QRC.
4. Answer the open questions above from it, then write the mock server
   and the client against it.

## Also possible, not pursued

The opposite direction needs no work on the unit: nestlocal already
publishes the thermostat over MQTT (`docs/protocols/index.html`), and
OpenAVC has an MQTT transport and drivers in YAML, so an OpenAVC driver
could put the thermostat on any OpenAVC panel.
