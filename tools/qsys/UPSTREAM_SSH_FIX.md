# Draft: upstream fix for key-only SSH on the gen 3 (not posted)

For Erik to post on github.com/unfr0zen/unfr0zen himself. Nothing here has
been sent anywhere.

The fix is one commit on `claude/magical-edison-6wrezw`:
"app: write a key-only ssh login the gen 3's openssh 6.0 accepts". It
touches only `crates/unfr0zen-core/src/diamond3/rootfs.rs` and its tests
in `crates/unfr0zen-core/src/patch.rs`, so it applies to upstream's `main`
on its own. A pull request needs a branch off upstream's `main` holding
just that commit (in a fork of upstream: `git cherry-pick <sha>`).

---

**Title:** app: write a key-only ssh login the gen 3's openssh 6.0 accepts

**Body:**

With SSH on and password login off (the app's default since #54), the
patch writes `PermitRootLogin prohibit-password`. The 3rd generation's
stock OpenSSH 6.0 doesn't know that name (it arrived in OpenSSH 7.0), so
`sshd -t` fails (`unsupported option "prohibit-password"`, exit 255), sshd
exits at boot and port 22 refuses connections.

This writes `without-password`, the name 6.0 knows for the same setting
(newer releases still accept it), and treats the old line as the SSH form
so that patching again repairs units patched with v0.5.0.

Found on a 3rd generation unit, firmware 6.3-5, patched with v0.5.0;
setting `without-password` by hand made sshd start and key login work.
`cargo test -p unfr0zen-core` passes, with a test for the repair.

---

Notes for Erik (not part of the PR):

- Upstream's PR template, if any, should be checked when posting.
- Until upstream ships a fix, patching with the upstream exe and password
  login off brings the bug back; the `sed` line in HANDOFF.md repairs it
  from the USB shell.
