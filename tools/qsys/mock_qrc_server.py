#!/usr/bin/env python3
"""
mock_qrc_server.py - tiny fake Q-SYS Core for testing qrc_discover.py offline.

The response shapes below are ASSUMED from the QRC docs (Name/Type/Value/
ValueMin/ValueMax/Direction/String). They test the script's logic only; they do
NOT prove a real Core returns these fields. Verify with `qrc_discover.py --raw`
against a real Core or Designer emulation.

Usage: python3 mock_qrc_server.py [port]   (default 1710; use e.g. 11710 if busy)
"""
import json
import socketserver
import sys
import threading

STATE = {
    ("NEST_Zone1", "gain"): -12.0,
    ("NEST_Zone1", "mute"): False,
    ("NEST_Zone1", "source"): 2,
    ("NEST_Zone1", "page"): False,
    ("NEST_Status", "msg"): "Ready",
}
LOCK = threading.Lock()
POLLS = {"n": 0}

COMPONENTS = [
    {"Name": "NEST_Zone1", "Type": "gain", "Properties": []},
    {"Name": "NEST_Status", "Type": "custom_controls", "Properties": []},
    {"Name": "Other_Component", "Type": "gain", "Properties": []},
]

CONTROLS = {
    "NEST_Zone1": [
        {"Name": "gain", "Type": "Float", "Value": -12.0, "ValueMin": -100.0, "ValueMax": 20.0,
         "String": "-12.0dB", "Position": 0.77, "Direction": "Read/Write"},
        {"Name": "mute", "Type": "Boolean", "Value": False, "ValueMin": 0, "ValueMax": 1,
         "String": "unmuted", "Position": 0.0, "Direction": "Read/Write"},
        {"Name": "source", "Type": "Integer", "Value": 2, "ValueMin": 1, "ValueMax": 4,
         "String": "2", "Position": 0.33, "Direction": "Read/Write"},
        {"Name": "page", "Type": "Trigger", "Value": False, "String": "", "Position": 0.0,
         "Direction": "Read/Write"},
    ],
    "NEST_Status": [
        {"Name": "msg", "Type": "Text", "Value": "Ready", "String": "Ready", "Position": 0.0,
         "Direction": "Read Only"},
    ],
    "Other_Component": [
        {"Name": "gain", "Type": "Float", "Value": 0.0, "ValueMin": -100.0, "ValueMax": 20.0,
         "String": "0.0dB", "Position": 1.0, "Direction": "Read/Write"},
    ],
}


class Handler(socketserver.BaseRequestHandler):
    def send(self, obj):
        self.request.sendall(json.dumps(obj).encode() + b"\x00")

    def handle(self):
        self.send({"jsonrpc": "2.0", "method": "EngineStatus",
                   "params": {"State": "Active", "DesignName": "mock", "IsEmulator": True}})
        buf = b""
        while True:
            try:
                data = self.request.recv(4096)
            except OSError:
                return
            if not data:
                return
            buf += data
            while b"\x00" in buf:
                raw, buf = buf.split(b"\x00", 1)
                if not raw.strip():
                    continue
                msg = json.loads(raw)
                self.dispatch(msg)

    def dispatch(self, msg):
        m, rid, p = msg.get("method"), msg.get("id"), msg.get("params")
        if m == "NoOp" or rid is None:
            return
        if m == "Logon":
            res = True
        elif m == "Component.GetComponents":
            res = COMPONENTS
        elif m == "Component.GetControls":
            name = p["Name"]
            with LOCK:
                ctrls = [dict(c, Value=STATE.get((name, c["Name"]), c["Value"]))
                         for c in CONTROLS.get(name, [])]
            res = {"Name": name, "Controls": ctrls}
        elif m == "Component.Get":
            name = p["Name"]
            with LOCK:
                res = {"Name": name, "Controls": [
                    {"Name": c["Name"], "Value": STATE.get((name, c["Name"]))}
                    for c in p["Controls"]]}
        elif m == "Component.Set":
            name = p["Name"]
            with LOCK:
                for c in p["Controls"]:
                    if (name, c["Name"]) not in STATE:
                        self.send({"jsonrpc": "2.0", "id": rid,
                                   "error": {"code": 8, "message": "unknown control"}})
                        return
                    STATE[(name, c["Name"])] = c["Value"]
            res = True
        elif m == "ChangeGroup.AddComponentControl":
            res = {"Id": p["Id"], "Count": len(p["Component"]["Controls"])}
        elif m == "ChangeGroup.Poll":
            with LOCK:
                POLLS["n"] += 1
                n = POLLS["n"]
            changes = []
            if n % 2 == 0:
                changes.append({"Component": "NEST_Zone1", "Name": "gain",
                                "Value": -12.0 + n, "String": "%.1fdB" % (-12.0 + n)})
            res = {"Id": p["Id"], "Changes": changes}
        else:
            self.send({"jsonrpc": "2.0", "id": rid,
                       "error": {"code": -32601, "message": "Method not found"}})
            return
        self.send({"jsonrpc": "2.0", "id": rid, "result": res})


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 1710
    with Server(("127.0.0.1", port), Handler) as srv:
        print("mock QRC server on 127.0.0.1:%d" % port, flush=True)
        srv.serve_forever()
