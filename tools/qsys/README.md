# Q-SYS tools

Host-side tools for the Nest-as-Q-SYS-control-surface work (see
[HANDOFF.md](HANDOFF.md)). Python 3.8+, stdlib only.

- `qrc_discover.py`: connects to a Core or Designer's emulation over QRC,
  lists the `NEST_*` Named Components and the widget each control would get.
- `mock_qrc_server.py`: a fake Core for testing offline. Its replies are
  **assumptions**; passing against it proves the script's logic only.
- The Rust client in `crates/qsys` does the same discovery:
  `cargo run -p qsys --example discover -- HOST [same options]`.

## Testing against Designer emulation

The goal is to find out what a real Core returns, so `pick_widget()` (Python)
and `widget::pick` (Rust) match it.

### 1. Build a test design

Add these components to a new design. For each one, set the **Code Name**
in Properties and set **Script Access** to **External** (or All):

| Code Name | Component | What it checks |
|---|---|---|
| `NEST_Zone1` | Gain | Float with range (gain), Boolean (mute) |
| `NEST_Panel` | Custom Controls, with: a Toggle button, a Momentary button, a Trigger button, a Text box, a display text, an LED, a Meter | widgets read from Custom Controls' names |
| `NEST_Combo` | Text Controller running `combo_box.lua` | what a combo box reports (a `Choices` list?) |
| `NEST_Knob` | Gain, set to knob style | whether a knob differs from a fader over QRC |
| `NEST_Hidden` | Gain, **Script Access = None** | that it is left out |
| `Other_Gain` | Gain, Script Access = External | the prefix filter |

Save, then switch Designer to **Emulate** mode.

### 2. Run discovery

On the PC running Designer (Windows: use `py` instead of `python3`; in
PowerShell, `> raw.txt` saves UTF-16, which is fine to send back as is):

```sh
python3 qrc_discover.py 127.0.0.1 --raw > raw.txt
python3 qrc_discover.py 127.0.0.1 --all
python3 qrc_discover.py 127.0.0.1 --watch 30     # move controls in Designer meanwhile
python3 qrc_discover.py 127.0.0.1 --set NEST_Zone1.gain=-20 --set NEST_Zone1.mute=true
```

QRC listens on TCP 1710. I believe emulation answers on 127.0.0.1, but I
haven't checked that. If the connection is refused, try the PC's LAN address,
and allow Designer through the Windows firewall if it asks.

### 3. What to look for and send back

`raw.txt` is the main thing. Beyond it:

- Do controls have `Type`, `ValueMin`, `ValueMax`? What exact `Type` strings
  appear (Float/Integer/Boolean/Trigger/Text/…)?
- Is a Boolean's `Value` `true/false` or `0/1`?
- What does the combo box report? Is there a `Choices` field? (Answered
  for everything else: see HANDOFF.md, "Q-SYS QRC".)
- Does the knob-style Gain differ from a fader in any field?

## Offline

```sh
python3 mock_qrc_server.py 11710 &
python3 qrc_discover.py 127.0.0.1 --port 11710 --watch 5
```
