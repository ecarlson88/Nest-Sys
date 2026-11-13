#!/usr/bin/env python3
"""
mock_qrc_server.py - tiny fake Q-SYS Core for testing qrc_discover.py offline.

The design below is what Designer's emulation returned for a test design
(Text controls have no Value, only String). How it answers Set, Get and change
groups is assumed; passing against it tests the script's logic only.

Usage: python3 mock_qrc_server.py [port]   (default 1710; use e.g. 11710 if busy)
"""
import json
import socketserver
import sys
import threading

STATE = {
    ('NEST_Zone1', 'bypass'): False,
    ('NEST_Zone1', 'gain'): 0.0,
    ('NEST_Zone1', 'invert'): False,
    ('NEST_Zone1', 'mute'): False,
    ('NEST_Panel', 'led.1'): False,
    ('NEST_Panel', 'meter.1'): -120.0,
    ('NEST_Panel', 'momentary.1'): False,
    ('NEST_Panel', 'text.1'): '',
    ('NEST_Panel', 'text.display.1'): '',
    ('NEST_Panel', 'toggle.1'): False,
    ('NEST_Panel', 'trigger.1'): 0.0,
}
LOCK = threading.Lock()
POLLS = {"n": 0}

# Shapes as Designer's emulation returned them (crates/qsys/tests/data/designer).
COMPONENTS = [
    {"Name": "NEST_Panel", "Type": "custom_controls", "Properties": []},
    {"Name": "Other_ Gain", "Type": "gain", "Properties": []},
    {"Name": "NEST_Zone1", "Type": "gain", "Properties": []},
    {"Name": "Nest_Combo", "Type": "device_controller_script", "Properties": []},
]

CONTROLS = {
    "NEST_Zone1": [
        {
            "Name": "bypass",
            "Type": "Boolean",
            "Value": False,
            "String": "no",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "gain",
            "Type": "Float",
            "Value": 0.0,
            "ValueMin": -100.0,
            "ValueMax": 20.0,
            "StringMin": "-100dB",
            "StringMax": "20.0dB",
            "String": "0dB",
            "Position": 0.83333331,
            "Direction": "Read/Write"
        },
        {
            "Name": "invert",
            "Type": "Boolean",
            "Value": False,
            "String": "normal",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "mute",
            "Type": "Boolean",
            "Value": False,
            "String": "unmuted",
            "Position": 0.0,
            "Direction": "Read/Write"
        }
    ],
    "NEST_Panel": [
        {
            "Name": "led.1",
            "Type": "Boolean",
            "Value": False,
            "String": "false",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "meter.1",
            "Type": "Float",
            "Value": -120.0,
            "ValueMin": -120.0,
            "ValueMax": 20.0,
            "StringMin": "-120dB",
            "StringMax": "20.0dB",
            "String": "-120dB",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "momentary.1",
            "Type": "Boolean",
            "Value": False,
            "String": "false",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "text.1",
            "Type": "Text",
            "String": "",
            "Direction": "Read/Write"
        },
        {
            "Name": "text.display.1",
            "Type": "Text",
            "String": "",
            "Direction": "Read/Write"
        },
        {
            "Name": "toggle.1",
            "Type": "Boolean",
            "Value": False,
            "String": "false",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "trigger.1",
            "Type": "Trigger",
            "Value": 0.0,
            "Direction": "Read/Write"
        }
    ],
    "Other_ Gain": [
        {
            "Name": "bypass",
            "Type": "Boolean",
            "Value": False,
            "String": "no",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "gain",
            "Type": "Float",
            "Value": 0.0,
            "ValueMin": -100.0,
            "ValueMax": 20.0,
            "StringMin": "-100dB",
            "StringMax": "20.0dB",
            "String": "0dB",
            "Position": 0.83333331,
            "Direction": "Read/Write"
        },
        {
            "Name": "invert",
            "Type": "Boolean",
            "Value": False,
            "String": "normal",
            "Position": 0.0,
            "Direction": "Read/Write"
        },
        {
            "Name": "mute",
            "Type": "Boolean",
            "Value": False,
            "String": "unmuted",
            "Position": 0.0,
            "Direction": "Read/Write"
        }
    ]
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
                ctrls = []
                for c in CONTROLS.get(name, []):
                    c = dict(c)
                    v = STATE.get((name, c["Name"]))
                    if "Value" in c:
                        c["Value"] = v
                    else:
                        c["String"] = v
                    ctrls.append(c)
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
                                "Value": -20.0 + n, "String": "%.1fdB" % (-20.0 + n)})
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
