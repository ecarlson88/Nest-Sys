# Extron: findings and handoff

Notes for whoever builds the Extron control surface. Nothing here has been
built, run or tested. Extron's own site refused automated requests, so
what is said about Extron comes from third-party copies of its manuals,
its press articles, and memory. Statements marked **[unverified]** must be
checked in Global Scripter's own documentation (its Help menu holds the
ControlScript reference) before anything is built on them.

## How an Extron system is programmed

- **GUI Designer** lays out a TouchLink panel's pages; each control on
  them has an ID.
- **Global Configurator** (Plus, or Professional for larger systems; ECP
  certification) configures a system without code: it attaches actions to
  those control IDs. GUI Configurator's "Where Used" window tracks which
  IDs are used, and "Show Control Info" shows a button's ID
  ([TouchLink best practices][bp], [TLP Pro 725T manual][tlp]).
- **Global Scripter** programs Pro Series processors in Python 3 with
  Extron's **ControlScript** library ([Global Scripter][gs],
  [Lawrenceville case study][lv]). Access is limited to programmers Extron
  has authorised; it has its own licensing, like GC's. Whether GC Pro
  certification is enough is to be checked.

In ControlScript, from memory **[unverified]**: a panel is a `UIDevice`,
and its controls are objects made from it by GUI Designer ID,
`Button(tlp, 8001)`, `Level(tlp, 8002)`, `Label(tlp, 8003)`, and on
panels that have one, `Knob(tlp, ID)`. Buttons report `Pressed`,
`Released`, `Held`, `Repeated` and `Tapped`; a Knob reports `Turn`.
Handlers are attached with the `@event` decorator. The processor can run
a TCP server (`EthernetServerInterfaceEx`).

## The plan: the Nest as a Global Scripter panel

Two halves, both written here:

1. **`nest_panel.py`**, a ControlScript module the programmer adds to the
   Global Scripter project. It listens for Nests, and offers the Nest's
   controls by ID with the same method names and events as a TLP's, so
   programming a Nest reads like programming a TLP:

   ```python
   nest = NestPanel('Lobby', port=5100)       # sketch, names not final
   volume = nest.Knob(8001, name='Volume', min=0, max=100)
   mute = nest.Button(8002, name='Mute', kind='toggle')

   @event(volume, 'Turn')
   def volume_turned(control, state): ...
   ```

   Whether ControlScript's own `@event` accepts objects that are not its
   own is **[unverified]**; if not, the module offers its own decorator of
   the same shape.

2. **`nest-extron`** on the unit, a client of that module built on the
   surface (as `nest-surface` is for Q-SYS): it connects, receives the
   controls, draws one per screen, and sends the ring and click back.

### The connection

The Nest connects to the processor, as it connects to a Q-SYS Core: the
processor needs no Nest's address, a processor can serve several Nests,
and the surface's runner already connects and retries. Each Nest sends a
name in its config (`Lobby`), which picks the `NestPanel` it belongs to,
as a TLP's alias does.

A proposed protocol, line-delimited JSON over TCP (ControlScript is
Python 3, whose standard library has `json` **[unverified that it is
available on the processor]**):

| Direction | Message | Meaning |
|---|---|---|
| Nest → processor | `{"hello": "Lobby", "version": 1, "password": "..."}` | which panel this is |
| processor → Nest | `{"controls": [{"id": 8001, "type": "knob", "name": "Volume", "min": 0, "max": 100}, ...]}` | the controls, in order |
| processor → Nest | `{"id": 8001, "level": 40}`, `{"id": 8002, "state": 1}`, `{"id": 8003, "text": "..."}`, `{"id": 8002, "visible": false}` | the script set a control |
| Nest → processor | `{"id": 8002, "event": "pressed"}`, `"released"`, `"held"`, `{"id": 8001, "event": "turn", "by": 3}`, `{"id": 8004, "event": "changed", "value": 2}` | the ring and click |
| both | `{"ping": n}` / `{"pong": n}` | keep-alive |

The processor owns all state: the Nest shows what the script sets and
reports what was touched. A toggle lights only when the script sets its
state, as on a TLP.

### Mapping onto the surface

The surface's widgets are `qsys::Widget` (`crates/qsys/src/widget.rs`):

| Control | Surface widget | Events |
|---|---|---|
| Knob | `Dial`, level from the script; the ring turns it | `turn` (detents) |
| Level, set by the script only | `Meter` | |
| Slider-like level the Nest sets | `Dial` | `changed` with the value |
| Button, momentary | `Momentary` | `pressed`, `released`, `held` |
| Button, toggle | `Toggle`, lit by `state` | `pressed`, `released` |
| Button, one-shot | `Button` | `pressed`, `released` |
| Choice | `Picker` with `choices` | `changed` with the index |
| Label | `Label` | |
| LED | `Indicator` | |

The Knob is the natural fit for the ring: a TLP Pro's rotary knob
reports turns in the same way **[unverified]**, so a script written for
one could drive the other.

### The seam in the surface

The same as for OpenAVC (`tools/openavc/HANDOFF.md` on
`feature/OpenAVC`, "The seam in the surface"): the model types move out of
`qsys`, and the runner drives a backend interface that `qsys`, `openavc`
and `extron` each implement. This work is shared and done once, on
whichever branch gets there first.

### Testing without a processor

Both ends are ours, so nearly all of it can be tested on a PC:
`mock_processor.py` runs `nest_panel.py` with small stand-ins for the
ControlScript classes it uses, and the surface's simulator
(`cargo run -p surface --bin sim`) connects to it as the Nest would. What
needs a processor is only the ControlScript glue: the server class, the
`@event` question, and how the module loads in a Global Scripter project.

## Global Configurator

Assigning a Nest's IDs inside GC itself, as for a TLP, is most likely not
possible: GC knows Extron's own panels, and the TouchLink protocol is not
public. No sign was found that GC Plus or Pro runs custom code or accepts
other panels ([GC Pro page][gcpro]); that is not confirmed either way.

One idea to check in GC Pro, untested: if GC can control a generic
Ethernet device and act on strings it receives from it, the Nest could be
added as such a device, with each control sending a fixed string
(`8002 PRESSED`) that GC acts on. That would give GC programmers IDs of a
sort, without Global Scripter, but likely no feedback to the screen.

## Open questions

1. **Can Erik get into Global Scripter** with his certification, or what
   it takes.
2. **Where each ID's control is defined**: in the script, sent to the Nest
   when it connects (recommended: one place, no file to copy to the unit),
   or in a layout file on the Nest, as a GUI Designer file is for a TLP.
3. The **[unverified]** ControlScript points above: the class and event
   names, the Knob, `EthernetServerInterfaceEx`, `@event` on our own
   objects, `json` on the processor.
4. The GC Pro string idea above, which Erik can check in GC Pro himself.
5. Whether the connection needs more than a shared password (ControlScript
   may offer TLS servers **[unverified]**).

## First session with Global Scripter

1. Read the ControlScript reference in Global Scripter's Help and settle
   question 3.
2. Write `nest_panel.py` and `mock_processor.py`, and the `extron` client
   against them; test with the simulator.
3. With a processor: load the module in a small project with one Knob,
   one toggle and one label, and run the Nest against it.

[bp]: https://manualmachine.com/extronelectronics/touchlinkseries/21518321-best-practices-manual
[tlp]: https://manualslib.com/manual/1357611/Extron-Electronics-Tlp-Pro-725t.html?page=23
[gs]: https://azwebdev.extron.com/product/globalscripter
[lv]: https://azwebdev.extron.com/article/lawrencevillepython
[gcpro]: https://azwebdev.extron.com/product/gcpro
