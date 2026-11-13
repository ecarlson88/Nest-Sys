#!/usr/bin/env python3
"""
qrc_discover.py - discover NEST_* Named Components on a Q-SYS Core (or Designer
emulator) over QRC and show which widget the Nest UI would use for each control.

Stdlib only. Python 3.8+.

Usage:
  python3 qrc_discover.py CORE_IP
  python3 qrc_discover.py CORE_IP --prefix NEST_ --user admin --password 1234
  python3 qrc_discover.py CORE_IP --raw        # dump raw GetControls JSON (check field names!)
  python3 qrc_discover.py CORE_IP --watch 30   # then follow changes for 30 s
  python3 qrc_discover.py CORE_IP --set NEST_Zone1.gain=-20 --set NEST_Zone1.mute=true

QRC basics (https://help.qsys.com/Content/External_Control_APIs/QRC/QRC_Commands.htm):
  TCP port 1710, JSON-RPC 2.0, every message terminated with a NUL byte.
  Components must have a Code Name AND Script Access = External or All.
"""
import argparse
import json
import socket
import sys
import time


class QRCError(Exception):
    pass


class QRC:
    def __init__(self, host, port=1710, timeout=5.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.settimeout(timeout)
        self.buf = b""
        self.next_id = 1
        self.notifications = []  # unsolicited messages (e.g. EngineStatus)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def _read_msg(self):
        while b"\x00" not in self.buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("connection closed by Core")
            self.buf += chunk
        raw, self.buf = self.buf.split(b"\x00", 1)
        raw = raw.strip()
        return json.loads(raw.decode("utf-8")) if raw else None

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self.sock.sendall(json.dumps(msg).encode("utf-8") + b"\x00")

    def call(self, method, params=None):
        rid = self.next_id
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        self.sock.sendall(json.dumps(msg).encode("utf-8") + b"\x00")
        while True:
            m = self._read_msg()
            if m is None:
                continue
            if m.get("id") == rid:
                if "error" in m:
                    raise QRCError("%s -> %s" % (method, m["error"]))
                return m.get("result")
            self.notifications.append(m)


def pick_widget(c):
    """Map one control (dict from Component.GetControls) to a UI widget.

    Returns (widget, note). Uses Type/ValueMin/ValueMax/Direction when present
    and falls back to the Python type of Value. 'note' flags missing metadata so
    the test run shows what the Core really provides.
    """
    t = str(c.get("Type", "")).strip().lower()
    d = str(c.get("Direction", "")).strip().lower()
    v = c.get("Value")
    vmin, vmax = c.get("ValueMin"), c.get("ValueMax")
    read_only = d.startswith("read") and "write" not in d
    notes = []
    if not t:
        notes.append("no Type field")
    if not d:
        notes.append("no Direction field")

    # Decide the data kind
    # Type strings seen from Designer: Boolean, Float, Integer, Text, Trigger, Status.
    if t == "boolean":
        kind = "bool"
    elif t == "trigger":
        kind = "trigger"
    elif t in ("text", "status"):
        kind = "text"
    elif t in ("float", "integer"):
        kind = "number"
    elif isinstance(v, bool):
        kind = "bool"
    elif isinstance(v, (int, float)):
        kind = "number"
    elif isinstance(v, str):
        kind = "text"
    else:
        kind = "unknown"

    if kind == "bool":
        return ("indicator" if read_only else "toggle"), "; ".join(notes)
    if kind == "trigger":
        return ("-" if read_only else "button"), "; ".join(notes)
    if t == "status":
        return "label", "; ".join(notes)
    if kind == "text":
        return ("label" if read_only else "text-entry (picker)"), "; ".join(notes)
    if kind == "number":
        have_range = isinstance(vmin, (int, float)) and isinstance(vmax, (int, float)) and vmax > vmin
        if read_only:
            return ("meter" if have_range else "label"), "; ".join(notes)
        if have_range:
            return "dial", "; ".join(notes)
        notes.append("no ValueMin/ValueMax")
        return "number-entry", "; ".join(notes)
    notes.append("unrecognised")
    return "label", "; ".join(notes)


def fmt(x, width):
    s = "" if x is None else str(x)
    return (s[: width - 1] + "~") if len(s) > width else s.ljust(width)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("host")
    ap.add_argument("--port", type=int, default=1710)
    ap.add_argument("--prefix", default="NEST_", help="Code Name prefix to keep (default NEST_)")
    ap.add_argument("--user")
    ap.add_argument("--password", default="")
    ap.add_argument("--raw", action="store_true", help="print raw GetComponents/GetControls JSON")
    ap.add_argument("--watch", type=float, default=0, metavar="SEC", help="follow changes for SEC seconds")
    ap.add_argument("--all", action="store_true", help="ignore --prefix and list every named component")
    ap.add_argument("--set", action="append", default=[], metavar="COMP.CTRL=VALUE",
                    help="Component.Set before listing (repeatable); VALUE is JSON if it parses, else a string")
    args = ap.parse_args()

    q = QRC(args.host, args.port)
    try:
        if args.user:
            q.call("Logon", {"User": args.user, "Password": args.password})
            print("logged on as", args.user)

        for spec in args.set:
            target, eq, text = spec.partition("=")
            comp, _, ctrl = target.partition(".")  # control names have dots (text.1)
            if not eq or not comp or not ctrl:
                print("error: --set wants COMP.CTRL=VALUE, got %r" % spec, file=sys.stderr)
                return 2
            try:
                value = json.loads(text)
            except ValueError:
                value = text
            q.call("Component.Set", {"Name": comp, "Controls": [{"Name": ctrl, "Value": value}]})
            print("set %s.%s = %r" % (comp, ctrl, value))

        # The QRC docs' example sends a dummy "params"; follow it for safety.
        comps = q.call("Component.GetComponents", "test") or []
        if args.raw:
            print("--- GetComponents raw ---")
            print(json.dumps(comps, indent=2))
        names = [c.get("Name", "") for c in comps]
        keep = names if args.all else [n for n in names if n.startswith(args.prefix)]
        print("%d named components in design, %d match %s" % (
            len(names), len(keep), "(all)" if args.all else repr(args.prefix)))
        if not keep:
            print("Nothing to show. Check: Code Name set, Script Access = External/All, prefix.")
            return 1

        discovered = {}
        for name in keep:
            res = q.call("Component.GetControls", {"Name": name}) or {}
            ctrls = res.get("Controls", [])
            discovered[name] = ctrls
            if args.raw:
                print("--- GetControls raw: %s ---" % name)
                print(json.dumps(res, indent=2))
            print("\n== %s  (%d controls)" % (name, len(ctrls)))
            print(fmt("control", 26) + fmt("type", 9) + fmt("value", 12) + fmt("min", 8)
                  + fmt("max", 8) + fmt("dir", 11) + fmt("widget", 20) + "notes")
            for c in ctrls:
                w, note = pick_widget(c)
                print(fmt(c.get("Name"), 26) + fmt(c.get("Type"), 9) + fmt(c.get("String", c.get("Value")), 12)
                      + fmt(c.get("ValueMin"), 8) + fmt(c.get("ValueMax"), 8)
                      + fmt(c.get("Direction"), 11) + fmt(w, 20) + note)

        if args.watch > 0:
            gid = "nest-discover"
            for name, ctrls in discovered.items():
                q.call("ChangeGroup.AddComponentControl", {
                    "Id": gid,
                    "Component": {"Name": name, "Controls": [{"Name": c["Name"]} for c in ctrls]},
                })
            print("\nwatching for %.0fs (Ctrl-C to stop)..." % args.watch)
            end = time.time() + args.watch
            last_noop = time.time()
            while time.time() < end:
                res = q.call("ChangeGroup.Poll", {"Id": gid}) or {}
                for ch in res.get("Changes", []):
                    print("  %s.%s = %s" % (ch.get("Component", "?"), ch.get("Name"),
                                            ch.get("String", ch.get("Value"))))
                if time.time() - last_noop > 20:
                    q.notify("NoOp", {})
                    last_noop = time.time()
                time.sleep(1.0)
        return 0
    except (QRCError, ConnectionError, OSError) as e:
        print("error:", e, file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        return 0
    finally:
        q.close()


if __name__ == "__main__":
    sys.exit(main())
