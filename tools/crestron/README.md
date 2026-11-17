# Crestron on the Nest

Findings for using the Nest as a control surface for a Crestron control
system, alongside the Q-SYS surface in [`tools/qsys`](../qsys). This
folder holds notes only: nothing here has been built, run or tested, and
no Crestron processor is available yet. Details, sources and open
questions are in [HANDOFF.md](HANDOFF.md).

## The plan

The Nest talks to a **SIMPL Windows** program on a **4-Series** processor
through the **Intersystem Communication (XSIG)** symbol over TCP, the way
Home Assistant's Crestron integration does. To the programmer it behaves
like a touch panel: its controls are digital, analog and serial joins.

- A button press drives a digital join high, its release low; the same
  join from the program lights it.
- The ring sets an analog join (0-65535); the program's value on that
  join moves the level shown.
- A serial join from the program sets a label's text.

The rest of the room is programmed in SIMPL anyway, so the Nest's joins
are wired to the program's logic like any panel's.

## Decisions so far

- This branch, `feature/Crestron`, is based on `feature/qsys`, since the
  surface it builds on is only there.
- Crestron gets **a program of its own** on the unit (`nest-crestron`), as
  OpenAVC and Extron do, not a mode of `nest-surface`.
- **XSIG over TCP**, using only stock SIMPL symbols. Speaking CIP (the
  touch-panel protocol) was ruled out: it is not published, and reverse
  engineering it would leave the Nest at the mercy of firmware changes.
- SIMPL Windows on 4-Series. No processor yet.

## Proposed layout

Nothing below exists yet.

| Path | What |
|---|---|
| `crates/crestron` | XSIG encoding and decoding, the connection, and the Nest's layout of joins, mapped onto the surface's screens. The counterpart of `crates/qsys`. |
| `crates/surface` | Made independent of Q-SYS, so each client can drive it (shared with `feature/OpenAVC` and `feature/Extron`). |
| `crates/surface/src/bin/nest-crestron.rs` | The Crestron program on the unit, beside `nest-surface`. |
| `crates/surface/rootfs/nest-crestron.sh` | Its launcher, the same screen hand-over as `nest-surface.sh`. |
| `tools/crestron/mock_processor.py` | A stand-in processor speaking XSIG, so the Nest's side can be tested without one. |
| `tools/crestron/simpl/` | How to wire the Nest in a SIMPL program; later, if it proves possible, a user module that does the wiring. |
