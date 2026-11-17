# Extron on the Nest

Findings for using the Nest as a control surface for an Extron control
system, alongside the Q-SYS surface in [`tools/qsys`](../qsys). This
folder holds notes only: nothing here has been built, run or tested, and
no Extron processor is available yet. Details, sources and open questions
are in [HANDOFF.md](HANDOFF.md).

## The plan

The Nest acts as a panel for a **Global Scripter** program on an Extron
Pro Series processor, the way a TouchLink Pro panel does:

- A ControlScript (Python) module, shipped from this repository, is added
  to the Global Scripter project. It makes the Nest look like a panel: the
  programmer creates controls by ID, `Button(nest, 8001)`,
  `Level(nest, 8002)`, and handles their events as for a TLP.
- The Nest connects to the processor over TCP, draws each ID's control on
  the round screen, and reports the ring and click back as events.

Doing the same inside Global Configurator itself would most likely need
the Nest to pass as a real TouchLink panel, whose protocol is not public.
Whether GC Pro can reach the Nest some other way is open (HANDOFF.md,
"Global Configurator").

## Decisions so far

- This branch, `feature/Extron`, is based on `feature/qsys`, since the
  surface it builds on is only there.
- Extron gets **a program of its own** on the unit (`nest-extron`), as
  OpenAVC does, not a mode of `nest-surface`.
- Global Scripter, not Global Configurator. Access to Global Scripter is
  still to be confirmed (Erik is certified up to GC Pro).
- Where each ID's control is defined is **not decided**; the
  recommendation is in the script (HANDOFF.md, "Open questions").

## Proposed layout

Nothing below exists yet.

| Path | What |
|---|---|
| `tools/extron/nest_panel.py` | The ControlScript module: the TCP server on the processor and the Nest's controls by ID. |
| `tools/extron/mock_processor.py` | Runs the module on a PC with stand-ins for ControlScript, so both ends can be tested without a processor. |
| `crates/extron` | The Nest's client for the module's protocol, mapped onto the surface's screens. The counterpart of `crates/qsys`. |
| `crates/surface` | Made independent of Q-SYS, so each client can drive it (shared with OpenAVC: `tools/openavc` on `feature/OpenAVC`). |
| `crates/surface/src/bin/nest-extron.rs` | The Extron program on the unit, beside `nest-surface`. |
| `crates/surface/rootfs/nest-extron.sh` | Its launcher, the same screen hand-over as `nest-surface.sh`. |
