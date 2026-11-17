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

## Global Configurator: a control module for the Nest

GC programs devices through **control modules** (device drivers). Erik
confirmed in GC Pro (2026-11-17) that:

- a status a device's module reports can trigger actions, and
- a command can be sent to a device with a value taken from another
  device's status.

So a module for the Nest would make it an ordinary device in GC, with no
Global Scripter and no TouchLink:

| In GC | On the Nest |
|---|---|
| Status `Button 8002` = pressed / released / held | the click on that control |
| Status `Knob 8001` turned by N, or `Level 8001` = value | the ring on that control |
| Status `Choice 8004` = index | a choice picked on a wheel |
| Command `Set State 8002` (0/1) | a toggle lit or not |
| Command `Set Level 8001` (value, e.g. from a DSP's feedback) | a level shown |
| Command `Set Text 8003` | a label |

GC then does the rest as for any device: a Nest press triggers a DSP
mute, and the DSP's mute feedback is sent back to light the Nest's toggle.

### What decides it

Only real module files can tell:

1. **Can we write one?** Whether a module is readable source GC loads
   from a file, or compiled, encrypted or signed by Extron. If modules
   cannot be authored outside Extron, this route needs Extron to build it
   (one programmer has reported Extron building a missing module on
   request; not confirmed), or falls back to Global Scripter.
2. **Its shape:** how a module declares commands, statuses and their
   values; whether a status can carry an ID and a value (one status per
   control type, `Button` with an ID) or must be one per control
   (`Button 8001`, `Button 8002`, ... up to a fixed count); how it parses
   what the device sends; whether the device can send unprompted (pushed
   events, not only replies to polls).
3. **Its transport:** TCP client to the device (the processor connecting
   to the Nest) is assumed; whether it can be a server instead.

Erik can download modules. Wanted, to read (kept out of the repository;
they are Extron's):

- a simple network device with commands and status (a display or
  projector over TCP);
- a device that sends events on its own, ideally physical buttons GC
  triggers on (Shure MXA mute buttons, a keypad);
- a generic or custom TCP module, if Extron offers one.

### The Nest's side

The module's protocol is ours to choose, within what modules can parse.
Likely a plain-text, line-based one in the style of Extron's SIS (one
command or report a line, e.g. `8002 PRESSED`, `SET 8001 40`), served by
`nest-extron` on a TCP port, since GC's modules most likely connect to
the device **[unverified]**. Then the Nest is the server and the
processor the client, the opposite of the Global Scripter route below.

Which controls a Nest has must be known to both GC and the Nest. With a
module the Nest's controls are likely defined on the Nest (a layout file
in its config, as a GUI Designer file is for a TLP) and the module offers
generic IDs; or, if a module can send setup, GC defines them. That
answer also comes from the module files.

A rough GC route, before modules were known (kept for reference): adding
the Nest as a generic Ethernet device and acting on fixed strings it
sends. A real module is better: named commands and statuses, and
feedback to the screen.

## Second route: the Nest as a Global Scripter panel

If modules cannot be written, or fall short. Two halves, both written here:

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

## Open questions

1. **Can control modules be written outside Extron**, and their shape
   (above, "What decides it"). Needs real module files.
2. **Can Erik get into Global Scripter** with his certification, or what
   it takes (second route only).
3. **Where each ID's control is defined**: in the script, sent to the Nest
   when it connects (recommended: one place, no file to copy to the unit),
   or in a layout file on the Nest, as a GUI Designer file is for a TLP.
4. The **[unverified]** ControlScript points above: the class and event
   names, the Knob, `EthernetServerInterfaceEx`, `@event` on our own
   objects, `json` on the processor.
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
