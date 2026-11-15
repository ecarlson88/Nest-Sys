# Draft: upstream issue for unfr0zen (not posted)

A draft for Erik to edit and post on github.com/unfr0zen/unfr0zen himself.
Nothing here has been sent anywhere.

---

**Title:** Apps: install and launch other full-screen programs, as Doom is

**Body:**

Doom shows that a program can take the display and the ring from the
UNFR0ZEN screen and give them back. I'd like to make that general: an
**Apps** feature where Doom is the first app and others can be added
without unfr0zen having to build or ship them.

My use: a control surface for a Q-SYS audio system on a 3rd generation
unit (the ring sets levels, the click mutes). It works today, started by
hand over SSH with the screen hand-over Doom's launcher uses. I'd like
other people to be able to install it from the app, and others to be able
to do the same for their own programs.

**What I have in mind** (happy to change any of it):

- **An app bundle**: one file (say `.tar.gz`) holding a small manifest
  (`app.json`: id, name, version, the generations it runs on, the program
  to run, a title picture for its page) and the files to install. Its
  author builds it; unfr0zen only installs it.
- **In the app and the CLI**: an **Apps** section on the Patch page's
  Features, with a switch per app (Doom among them, still built in) and
  **Add app from file…** for a bundle; on the CLI, `--app FILE` (and
  `--doom` as now). As with Doom, running the patch again without an app
  removes it.
- **On the unit**: each app installed under one directory of its own; the
  UNFR0ZEN screen lists them under **Apps**, each with a page like Doom's
  (title, how to quit, Play and Back); Play leaves a request, and nestlocal
  runs the app with the same screen hand-over Doom's launcher uses. Doom's
  page and launcher would become this general one.
- **Settings**: an app keeps its own settings on the unit and edits them
  on its own screen, so nothing app-specific is needed in unfr0zen.
- **Later, not part of this**: an option to start an app at boot in place
  of the thermostat screen.

**Questions for you**

1. Would you take this as a pull request, and does this shape fit how you
   want the patch flow and the UNFR0ZEN screen to grow?
2. A bundle installs a program that runs as root. Is a clear warning when
   adding one enough, or would you want something more (a checksum shown,
   only bundles from known sources)?
3. Where should apps live on the unit? The data volume is mounted
   `noexec`, so programs would go on the root file system, as Doom's do.

I can do the work and test it on a 3rd generation unit (firmware 6.3-5).

---

Notes for Erik (not part of the issue):

- It names no personal details; the firmware version is the only unit
  detail.
- Point 3 is why our own program had to move to `/nestlabs/sbin`.
- The `prohibit-password` SSH bug (HANDOFF.md) is a separate, smaller
  issue worth filing on its own.
