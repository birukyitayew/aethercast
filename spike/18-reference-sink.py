#!/usr/bin/env python3
"""AetherCast reference sink — a Miracast sink you can point a source at.

This is the project's reference implementation of the *sink* side, for
testing a source without any hardware. It plays the role a TV plays:

  1. connects to the source's RTSP control port (the source is the RTSP
     server in Wi-Fi Display - the roles are inverted compared to plain
     RTSP),
  2. answers the source's M1 with M2,
  3. answers the source's M3 with the capabilities it can accept,
  4. accepts the source's M4 format selection,
  5. sends M6 SETUP with the RTP port it wants the media on, and M7 PLAY,
  6. **receives the RTP media** and writes the MPEG-TS to a file.

Step 6 is what makes it useful: the result is a real transport stream
that can be validated with ffprobe, so the whole path — capture, encode,
mux, PID map, RTP payload type, packetisation — is exercised end to end.

Usage: reference-sink.py [source-host] [control-port] [rtp-port] [out.ts]
"""

import socket
import struct
import sys
import time

WFD_PUBLIC = "org.wfa.wfd1.0"

# The capabilities we advertise: a primary sink with one MPEG-TS stream.
VIDEO_FORMATS = "32,640x480p30-1;16,1920x1080p60-1"
AUDIO_CODECS = "LPCM/44100/2;LPCM/48000/2"


def log(message):
    print(f"[reference-sink] {message}", flush=True)


def read_message(sock):
    """Read one complete RTSP message: headers plus any announced body."""
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
    control_port = int(sys.argv[2]) if len(sys.argv) > 2 else 7236
    rtp_port = int(sys.argv[3]) if len(sys.argv) > 3 else 50000
    out_path = sys.argv[4] if len(sys.argv) > 4 else "/tmp/opencode/sink/received.ts"

    # The RTP socket is bound first, so it is ready when the source starts
    # sending.
    rtp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    rtp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    rtp.bind(("0.0.0.0", rtp_port))
    rtp.settimeout(2)
    log(f"listening for RTP on {rtp_port}, writing to {out_path}")

    # The source may still be starting up, so retry rather than fail.
    sock = None
    for attempt in range(30):
        try:
            sock = socket.create_connection((host, control_port), timeout=5)
            break
        except OSError:
            time.sleep(1)
    if sock is None:
        log(f"no source appeared on {host}:{control_port}")
        return 1
    sock.settimeout(15)
    log(f"connected to the source at {host}:{control_port}")

    # M1: the source asks whether we speak WFD.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 1\r\nServer: ReferenceSink/1.0\r\nPublic: org.wfa.wfd1.0\r\n\r\n")
    log("-> M1 200 OK")

    # M2: we ask the same question in return.
    send(
        sock,
        f"OPTIONS rtsp://{host}:{control_port}/wfd RTSP/1.0\r\nCSeq: 2\r\n"
        f"Require: {WFD_PUBLIC}\r\nUser-Agent: ReferenceSink/1.0\r\n\r\n",
    )
    log("-> M2 OPTIONS")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")

    # M3: the source asks what we can accept.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    capabilities = (
        f"wfd_client_rtp_ports=RTP/AVP/UDP;unicast;rtp-port0={rtp_port};rtp-port1=0\r\n"
        f"wfd_video_formats={VIDEO_FORMATS}\r\n"
        f"wfd_audio_codecs={AUDIO_CODECS}\r\n"
        "wfd_uibc_capability=none\r\n"
        "wfd_content_protection=none\r\n"
    )
    send(
        sock,
        f"RTSP/1.0 200 OK\r\nCSeq: 3\r\nServer: ReferenceSink/1.0\r\n"
        f"Content-Type: application/wfd\r\nContent-Length: {len(capabilities)}\r\n\r\n{capabilities}",
    )
    log(f"-> M3 200 OK (capabilities, rtp-port0={rtp_port})")

    # M4: the source chooses a format.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 4\r\nServer: ReferenceSink/1.0\r\n\r\n")
    log("-> M4 200 OK")

    # M5: the source triggers the setup.
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    send(sock, "RTSP/1.0 200 OK\r\nCSeq: 5\r\nServer: ReferenceSink/1.0\r\n\r\n")
    log("-> M5 200 OK")

    # M6: we set the transport up.
    send(
        sock,
        f"SETUP rtsp://{host}:{control_port}/wfd RTSP/1.0\r\nCSeq: 6\r\n"
        f"Transport: RTP/AVP/UDP;unicast;client-port={rtp_port}-{rtp_port + 1}\r\n\r\n",
    )
    log("-> M6 SETUP")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    session = ""
    for line in request.split("\r\n"):
        if line.lower().startswith("session:"):
            session = line.split(":", 1)[1].strip()
    log(f"   session: {session}")

    # M7: we start playing.
    send(sock, f"PLAY rtsp://{host}:{control_port}/wfd RTSP/1.0\r\nCSeq: 7\r\nSession: {session}\r\n\r\n")
    log("-> M7 PLAY")
    request = read_message(sock)
    log(f"<- {request.splitlines()[0]}")
    log("the sink is playing; receiving media")

    # Receive the media: strip the 12 byte RTP header and keep the MPEG-TS.
    received = 0
    packets = 0
    payload_types = set()
    with open(out_path, "wb") as handle:
        deadline = time.time() + 30
        while time.time() < deadline:
            try:
                data, _ = rtp.recvfrom(65535)
            except socket.timeout:
                if received:
                    break
                continue
            packets += 1
            if len(data) >= 12 and (data[0] >> 6) == 2:
                payload_types.add(data[1] & 0x7F)
                data = data[12:]
            handle.write(data)
            received += len(data)
    log(f"received {packets} RTP packet(s), {received} bytes of MPEG-TS")
    if payload_types:
        log(f"RTP payload types: {sorted(payload_types)}")
    log(f"the transport stream is in {out_path}")
    sock.close()
    rtp.close()
    return 0 if received else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 - a test script reports, it does not raise
        log(f"failed: {type(exc).__name__}: {exc}")
        sys.exit(1)
