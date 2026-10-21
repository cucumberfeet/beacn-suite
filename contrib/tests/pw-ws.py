#!/usr/bin/env python3
# Minimal websocket client for the Pipeweaver API (stdlib only). Usage: ws.py '<json data>' [...]
import base64, json, os, socket, struct, sys
def connect(path="/api/websocket"):
    s = socket.create_connection(("127.0.0.1", 14565))
    key = base64.b64encode(os.urandom(16)).decode()
    s.send(f"GET {path} HTTP/1.1\r\nHost: localhost:14565\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
    buf = b""
    while b"\r\n\r\n" not in buf: buf += s.recv(1)
    return s
def send(s, text):
    data = text.encode(); hdr = bytearray([0x81]); n = len(data); mask = os.urandom(4)
    if n < 126: hdr.append(0x80 | n)
    elif n < 65536: hdr.append(0x80 | 126); hdr += struct.pack(">H", n)
    else: hdr.append(0x80 | 127); hdr += struct.pack(">Q", n)
    s.send(bytes(hdr) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
def rx(s, n):
    b = b""
    while len(b) < n: b += s.recv(n - len(b))
    return b
def recv(s):
    h = rx(s, 2); n = h[1] & 0x7f
    if n == 126: n = struct.unpack(">H", rx(s, 2))[0]
    elif n == 127: n = struct.unpack(">Q", rx(s, 8))[0]
    return rx(s, n).decode()
if __name__ == "__main__":
    s = connect()
    for i, a in enumerate(sys.argv[1:]):
        send(s, json.dumps({"id": i, "data": json.loads(a)}))
        while True:
            m = json.loads(recv(s))
            if m["id"] == i:
                print(json.dumps(m["data"]) if "Status" not in m["data"] else json.dumps(m["data"]["Status"], indent=1)); break
