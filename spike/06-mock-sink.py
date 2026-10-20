#!/usr/bin/env python3
"""A mock Wi-Fi Display sink, for testing the WFD RTSP handshake locally.

The real sink is the TV. This script plays its role so the handshake in
`crates/aethercastd/src/rtsp.rs` can be exercised and regression-tested
without any hardware, and so the exact message sequence AetherCast
produces can be inspected.

It follows the flow in the Wi-Fi Display specification:

    sink -> source : TCP connect
    source -> sink : M1  OPTIONS, Require: org.wfa.wfd1.0
    sink -> source : 200 OK
    sink -> source : M2  OPTIONS
    source -> sink : 200 OK
    source -> sink : M3  GET_PARAMETER
    sink -> source : 200 OK with the capabilities
    source -> sink : M4  SET_PARAMETER
    sink -> source : 200 OK
    source -> sink : M5  SET_PARAMETER (wfd_trigger_method: SETUP)
    sink -> source : 200 OK
    sink -> source : M6  SETUP (Transport: client-port=...)
    source -> sink : 200 OK with Session
    sink -> source : M7  PLAY
    source -> sink : 100 Continue

Usage: mock-sink.py [host] [port]
"""

import socket
import sys

WFD_PUBLIC = "org.wfa.wfd1.0"


def log(message):
    print(f"[mock-sink] {message}", flush=True)


def read_message(sock):
    """Read one complete RTSP message, headers plus any announced body."""
    raw = b""
    while not raw.endswith(b"\r\n\r\n") and not raw.endswith(b"\n\n"):
        chunk = sock.recv(1)
        if not chunk:
            raise ConnectionError("the source closed the connection")
        raw += chunk
    head = raw.decode(errors="replace")
    length = 0
    for line in head.split("\r\n"):
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key.strip().lower() == "content-length":
            length = int(value.strip())
    body = b""
    if length:
        while len(body) < length:
            chunk = sock.recv(length - len(body))
            if not chunk:
                break
            body += chunk
    return head + body.decode(errors="replace")


def send(sock, message):
    sock.sendall(message.encode())


def main():
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 7236

    sock = socket.create_connection((host, port), timeout=15)
    sock.settimeout(15)
    log(f"connected to {host}:{port}")

    # M1: the source asks whether we speak WFD.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 1\r\nServer: MockSink/1.0\r\nPublic: org.wfa.wfd1.0\r\n\r\n")
    log("-> M1 200 OK")

    # M2: we ask the same question in return.
    send(
        sock,
        f"OPTIONS rtsp://{host}:{port}/wfd RTSP/1.0\r\nCSeq: 2\r\n"
        f"Require: {WFD_PUBLIC}\r\nUser-Agent: MockSink/1.0\r\n\r\n",
    )
    log("-> M2 OPTIONS")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")

    # M3: the source asks what we can accept.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    capabilities = (
        "wfd_client_rtp_ports=RTP/AVP/UDP;unicast;rtp-port0=50000;rtp-port1=0\r\n"
        "wfd_video_formats=32,640x480p30-1;16,1920x1080p60-1\r\n"
        "wfd_audio_codecs=LPCM/44100/2;LPCM/48000/2\r\n"
        "wfd_uibc_capability=none\r\n"
        "wfd_content_protection=none\r\n"
    )
    send(
        sock,
        f"RTSP/1.0 200 OK\r\nCSeq: 3\r\nServer: MockSink/1.0\r\n"
        f"Content-Type: application/wfd\r\nContent-Length: {len(capabilities)}\r\n\r\n{capabilities}",
    )
    log("-> M3 200 OK (capabilities)")

    # M4: the source chooses a format.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 4\r\nServer: MockSink/1.0\r\n\r\n")
    log("-> M4 200 OK")

    # M5: the source triggers the setup.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 5\r\nServer: MockSink/1.0\r\n\r\n")
    log("-> M5 200 OK")

    # M6: we set the transport up.
    send(
        sock,
        f"SETUP rtsp://{host}:{port}/wfd RTSP/1.0\r\nCSeq: 6\r\n"
        f"Transport: RTP/AVP/UDP;unicast;client-port=50000-50001\r\n\r\n",
    )
    log("-> M6 SETUP (client-port=50000-50001)")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    session = ""
    for line in request.split("\r\n"):
        if line.lower().startswith("session:"):
            session = line.split(":", 1)[1].strip()
    log(f"   session: {session}")

    # M7: we start playing.
    send(sock, f"PLAY rtsp://{host}:{port}/wfd RTSP/1.0\r\nCSeq: 7\r\nSession: {session}\r\n\r\n")
    log("-> M7 PLAY")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")

    log("handshake complete: the sink is playing")
    # Keep the connection open and count the media that arrives.
    received = 0
    try:
        while True:
            data = sock.recv(4096)
            if not data:
                break
            received += len(data)
    except (socket.timeout, ConnectionError):
        pass
    log(f"received {received} bytes of media")
    sock.close()


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001 - a test script reports, it does not raise
        log(f"failed: {type(exc).__name__}: {exc}")
        sys.exit(1)
