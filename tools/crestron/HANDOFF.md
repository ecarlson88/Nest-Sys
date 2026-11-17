# Crestron: findings and handoff

Notes for whoever builds the Crestron control surface. Nothing here has
been built, run or tested, and no processor is available. The XSIG format
below is read from the code of the Home Assistant integration for the
XSIG symbol, [npope/home-assistant-crestron-component][ha] (Apache-2.0,
`custom_components/crestron/crestron.py` at `2b90696`, 2021-11-29), not
from Crestron documentation. Statements marked **[unverified]** must be
checked against SIMPL Windows' help or a processor.

## Routes considered

| Route | Verdict |
|---|---|
| **XSIG over TCP** (Intersystem Communication symbol, stock SIMPL) | **Chosen.** Stock symbols, a simple binary format already implemented in the open, and joins as a panel has. |
| CIP, the protocol touch panels and XPanels speak (TCP 41794) | Ruled out. Not published; reverse-engineered implementations exist ([crestronclient][cc], [python-cipclient][pc], [CommandFusion/CIP][cf]), but the Nest would depend on firmware details Crestron can change. 4-Series may also require authenticated or encrypted connections **[unverified]**. |
| A SIMPL# Pro library | Not needed: the program is in SIMPL Windows anyway. |

## XSIG over TCP

### The SIMPL side

As Home Assistant's integration is wired ([its README][ha]):

1. A **TCP/IP Client** device on the processor, set to the Nest's address
   and port; its `Connect` held high (or driven by logic).
2. One or more **Intersystem Communication** symbols (quick key `xsig`),
   their TX and RX wired to the client's serial signals.
3. The joins wired to the program's logic.

A caution from that README: on one XSIG with analogs or serials ahead of
digitals, the digitals are sent numbered after them (25 analogs then 10
digitals: the digitals arrive as 26-35, though labelled 1-10). Its advice,
which carries over: one XSIG for digitals, another for analogs and
serials.

### Which end connects

| | Processor: TCP/IP Client | Processor: TCP/IP Server |
|---|---|---|
| Who listens | the Nest | the processor |
| Address to configure | the Nest's, in SIMPL (needs a fixed address or a DHCP reservation) | the processor's, in the Nest's config |
| Proven with XSIG | yes, by Home Assistant | **[unverified]** |
| Fits the surface's runner | needs a listening mode | as is (it connects and retries, as to a Q-SYS Core) |

Leaning: the processor as client, as proven, unless a TCP/IP Server
symbol is shown to carry XSIG as well. Open question 1.

### The format

All values big-endian; `j` is the join number minus one; data bytes after
the first always have their top bit clear.

| Signal | Bytes | Layout |
|---|---|---|
| Digital | 2 | `10 c jjjjj`, `0 jjjjjjj`: `c` is the value **inverted** (0 = high, 1 = low); a 12-bit join |
| Analog | 4 | `11 vv 0 jjj`, `0 jjjjjjj`, `0 vvvvvvv`, `0 vvvvvvv`: a 10-bit join; a 16-bit value, its top two bits in the first byte, then 7 and 7 |
| Serial | 2 + text + 1 | `11001 jjj`, `0 jjjjjjj`, the text, `0xFF`: a 10-bit join; Home Assistant refuses text over 252 bytes |
| Update request | 1 | `0xFD`, sent by Home Assistant on each connection: asks the program for all its joins |
| Sync request | 1 | `0xFB`, from the program: asks the other end for all of its joins |

From the field widths: joins up to 4096 for digitals and 1024 for
analogs and serials **[unverified against SIMPL's own limits]**. Serial
text cannot hold `0xFF`; Home Assistant decodes it as UTF-8, while what
SIMPL sends is likely single bytes **[unverified]**.

The Nest's side therefore:

- on connecting (or being connected to), sends `0xFD` and takes the
  program's current joins;
- on `0xFB`, sends every join it drives (its levels and button states);
- sends a digital high on press, low on release; an analog when the ring
  moves a level;
- shows whatever the program sends on the joins in its layout.

## The Nest's layout

XSIG carries join numbers and values only: no names, no types, no ranges.
So the Nest needs a layout of its own, the counterpart of a touch panel's
project file: which joins it has, what each one is, what it is called,
in what order. A file in the program's config directory on the unit,
for example `/media/data/nest-crestron/layout.json`:

```json
{"screens": [
  {"name": "Room", "controls": [
    {"type": "level", "name": "Volume", "analog": 1, "mute": 1},
    {"type": "button", "name": "Display", "digital": 2},
    {"type": "choice", "name": "Source", "analog": 2,
     "choices": ["Laptop", "Wireless", "Camera"]},
    {"type": "label", "serial": 1}
  ]}
]}
```

Whether the layout could come from the program instead (sent on serial
joins at connection) is open question 2; the file is simpler and needs
nothing in SIMPL.

## Mapping onto the surface

The surface's widgets are `qsys::Widget` (`crates/qsys/src/widget.rs`).
Joins follow touch-panel conventions, the same join number in both
directions:

| Layout control | Joins | Surface widget | The Nest sends | The program sends |
|---|---|---|---|---|
| `level` | analog (+ digital mute) | `Dial`, the gain's volume knob with its mute folded in | analog 0-65535 as the ring turns; mute press/release | the level; the mute's state |
| `button` | digital | `Momentary` or `Button` | high on press, low on release | feedback: lit or not |
| `toggle` | digital | `Toggle`, lit by feedback | high on press, low on release | lit or not; the program does the toggling, as for a panel button |
| `choice` | analog | `Picker` with the layout's choices | the index picked | the current index |
| `meter` | analog | `Meter` | | the value |
| `label` | serial | `Label` | | the text |
| `indicator` | digital | `Indicator` | | on or off |

A level shows as 0-100% of 0-65535 by default; a layout may give a range
and unit to show instead (e.g. -100 to 12 dB) **[a choice to make]**.
An optional visibility digital per control, as touch-panel designs use,
can hide controls the program does not want shown.

## The seam in the surface

The same as for OpenAVC and Extron: the model types move out of `qsys`,
and the runner drives a backend interface that each client implements.
It is shared work, done once on whichever branch gets there first. The
Crestron client differs in one way: XSIG pushes changes, so like OpenAVC
it waits on the socket rather than polling as QRC does. If the processor
connects to the Nest, the runner also needs a listening mode.

## Testing without a processor

- **The codec** can be tested against Home Assistant's implementation,
  byte for byte, in both directions.
- **`mock_processor.py`** plays the program: connects (or listens), sends
  `0xFB` and joins, prints what the Nest sends. The surface's simulator
  (`cargo run -p surface --bin sim`) runs against it as the unit would.
- What needs a processor: the SIMPL wiring itself, the join numbering on
  real XSIGs, serial text encoding, and the open questions below.

## Open questions

1. **Which end connects**: whether a TCP/IP Server symbol carries XSIG
   as well as the client does (above, "Which end connects").
2. **Where the layout lives**: a file on the Nest (proposed), or sent by
   the program at connection.
3. **A user module**: whether the TCP/IP client and XSIGs can be packaged
   as a SIMPL user module (`.umc`) the programmer drops in. Device symbols
   such as a TCP/IP Client may not be allowed inside a module
   **[unverified]**; if not, the module would hold the XSIGs and the
   program the client. A SIMPL+ module with its own socket is another
   way **[unverified]**.
4. **4-Series**: that the TCP/IP Client and XSIG symbols behave on
   4-Series as Home Assistant's users found on earlier series
   **[unverified]**.
5. **Levels shown**: percent of 0-65535, or a range and unit from the
   layout.

## First session with a processor

1. A minimal SIMPL program: a TCP/IP Client to the Nest's address, one
   XSIG for digitals and one for analogs and serials, a few joins looped
   back (a digital press to its feedback, an analog to itself) and one
   serial from a string.
2. The Nest's layout naming those joins.
3. Check the format above, the join numbering, a long and a non-ASCII
   serial, and reconnecting after the Nest or the processor restarts.

[ha]: https://github.com/npope/home-assistant-crestron-component
[cc]: https://github.com/EneOli/crestronclient
[pc]: https://pypi.org/project/python-cipclient/
[cf]: https://github.com/CommandFusion/CIP
