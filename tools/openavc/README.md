# OpenAVC on the Nest

Findings for putting [OpenAVC](https://github.com/open-avc/openavc) on the
Nest, alongside the Q-SYS control surface in [`tools/qsys`](../qsys). This
folder holds notes only: nothing here has been built, run or tested.
Details, sources and open questions are in [HANDOFF.md](HANDOFF.md).

OpenAVC is an open-source AV control system (MIT licensed): a Python
server that talks to projectors, DSPs, switchers and the like, with a
web-based Programmer IDE and web touch panels. The notes are against
`open-avc/openavc` at `9c48255` (2026-11-16, version 0.40.0).

## The plan

A Nest control surface for an OpenAVC server running elsewhere. The round
screen, ring, hand-over and simulator in `crates/surface` are reused; a new
client speaks OpenAVC's panel protocol (`/ws`), and the unit is approved
once in the Programmer like any tablet.

## Decisions so far

- Running the OpenAVC server on the Nest itself is dropped: more work
  than it is worth.
- This branch, `feature/OpenAVC`, is based on `feature/qsys`, since the
  surface it builds on is only there.
- OpenAVC gets **a program of its own** on the unit, not a mode of
  `nest-surface`.
- Which OpenAVC pages the Nest shows is **not decided**
  (HANDOFF.md, "Open questions").
- There is no OpenAVC server to test against yet. One can be run from
  source on a PC (`pip install -e .`, then `openavc`; Programmer at
  `http://localhost:8080/programmer`).

## Proposed layout

Nothing below exists yet.

| Path | What |
|---|---|
| `crates/openavc` | Client for an OpenAVC server: panel check-in and approval, the `/ws` WebSocket, the UI definition and state, mapped onto the surface's screens. The counterpart of `crates/qsys`. |
| `crates/surface` | Unchanged in purpose, but its model and runner made independent of Q-SYS (HANDOFF.md, "The seam in the surface"), so both clients can drive it. |
| `crates/surface/src/bin/nest-openavc.rs` (or a crate of its own) | The OpenAVC program on the unit, beside `nest-surface`. |
| `crates/surface/rootfs/nest-openavc.sh` | Its launcher, the same screen hand-over as `nest-surface.sh`. |
| `tools/openavc` | These notes; later, a mock OpenAVC server for offline tests as `tools/qsys/mock_qrc_server.py` is for QRC. |
