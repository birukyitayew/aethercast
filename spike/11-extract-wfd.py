#!/usr/bin/env python3
"""Extract what Wi-Fi Display devices advertise from a monitor-mode capture.

A monitor-mode capture is the only way to see the management frames a
Miracast sink sends, but `tshark` does not decode the Wi-Fi Display
information element. This script parses the capture directly and prints
the raw bytes of every WFD element it finds, together with the P2P
attributes that identify the device.

For each device it reports:
  * the source address,
  * the P2P device address and the P2P primary device type, which is the
    "PC or a phone" category a sink may filter on,
  * the WFD information element, byte for byte.

Usage: extract-wfd.py <capture.pcap> [more.pcap ...]
"""

import struct
import sys
from collections import defaultdict

# 802.11 management frame subtypes we care about.
BEACON = 8
PROBE_REQUEST = 4
PROBE_RESPONSE = 5

# Wi-Fi Alliance OUI, used by both the P2P and the WFD information elements.
WFA_OUI = b"\x50\x6f\x9a"
P2P_IE_TYPE = 0x09
WFD_IE_TYPE = 0x0A

# P2P attribute ids.
P2P_ATTR_DEVICE_ADDRESS = 0
P2P_ATTR_DEVICE_TYPE = 2
P2P_ATTR_DEVICE_NAME = 9
P2P_ATTR_SSID = 8

# WFD subelement ids.
WFD_SUBELEM_DEVICE_INFO = 0

# P2P device type categories, from src/wps/wps_defs.h.
P2P_CATEGORY = {
    1: "computer",
    2: "input",
    3: "printer",
    4: "camera",
    5: "storage",
    6: "network-infrastructure",
    7: "display",
    8: "multimedia",
    9: "gaming",
    10: "phone",
    11: "audio",
}
P2P_SUBCATEGORY = {
    1: {1: "PC", 2: "server", 3: "media-center", 4: "ultra-mobile", 5: "notebook",
        6: "desktop", 7: "MID", 8: "netbook", 9: "tablet"},
    7: {1: "TV", 2: "projector", 3: "LCD", 4: "CRT", 5: "plasma", 6: "monitor"},
    10: {1: "phone", 2: "PDA", 3: "smartphone", 4: "handheld"},
}


def read_pcap(path):
    """Yield the raw bytes of every packet in a pcap file."""
    with open(path, "rb") as handle:
        data = handle.read()
    if len(data) < 24:
        return
    magic = data[:4]
    if magic == b"\xd4\xc3\xb2\xa1":
        endian, linktype = "<", struct.unpack("<I", data[20:24])[0]
    elif magic == b"\xa1\xb2\xc3\xd4":
        endian, linktype = ">", struct.unpack(">I", data[20:24])[0]
    elif magic == b"\x4d\x3c\xb2\xa1":
        endian, linktype = "<", struct.unpack("<I", data[20:24])[0]
    else:
        raise SystemExit(f"{path}: not a pcap file (magic {magic!r})")
    offset = 24
    while offset + 16 <= len(data):
        _, _, captured, _ = struct.unpack(endian + "IIII", data[offset : offset + 16])
        offset += 16
        frame = data[offset : offset + captured]
        offset += captured
        if linktype == 127:  # IEEE802_11_RADIO: strip the radiotap header
            if len(frame) < 4:
                continue
            radiotap_length = struct.unpack("<H", frame[2:4])[0]
            frame = frame[radiotap_length:]
        elif linktype != 105:  # IEEE802_11
            continue
        yield frame


def parse_ies(body):
    """Yield (id, payload) for every information element."""
    offset = 0
    while offset + 2 <= len(body):
        element_id = body[offset]
        length = body[offset + 1]
        payload = body[offset + 2 : offset + 2 + length]
        if len(payload) < length:
            return
        yield element_id, payload
        offset += 2 + length


def mac(raw):
    return ":".join(f"{b:02x}" for b in raw)


def parse_p2p_ie(payload):
    """Parse the attributes of a P2P information element."""
    attributes = {}
    offset = 0
    while offset + 3 <= len(payload):
        attr_id = payload[offset]
        length = struct.unpack(">H", payload[offset + 1 : offset + 3])[0]
        value = payload[offset + 3 : offset + 3 + length]
        if len(value) < length:
            break
        attributes.setdefault(attr_id, value)
        offset += 3 + length
    return attributes


def device_type_text(value):
    if len(value) < 4:
        return "unknown"
    category = value[2]
    subcategory = value[3]
    name = P2P_CATEGORY.get(category, f"category-{category}")
    sub = P2P_SUBCATEGORY.get(category, {}).get(subcategory)
    return f"{name}/{sub}" if sub else name


def describe_wfd(payload):
    """Describe a WFD information element, byte for byte."""
    lines = []
    offset = 0
    while offset + 3 <= len(payload):
        subelem_id = payload[offset]
        length = struct.unpack(">H", payload[offset + 1 : offset + 3])[0]
        body = payload[offset + 3 : offset + 3 + length]
        if len(body) < length:
            break
        hexbody = body.hex()
        lines.append(f"      subelement {subelem_id}: {hexbody}")
        if subelem_id == WFD_SUBELEM_DEVICE_INFO and len(body) >= 2:
            device_info = body[:2].hex()
            lines.append(f"        device information field: {device_info}")
            if len(body) >= 4:
                port = struct.unpack(">H", body[2:4])[0]
                lines.append(f"        control port: {port}")
            if len(body) >= 6:
                throughput = struct.unpack(">H", body[4:6])[0]
                lines.append(f"        maximum throughput: {throughput} Mbit/s")
        offset += 3 + length
    return lines


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    devices = defaultdict(
        lambda: {
            "frames": 0,
            "p2p_address": None,
            "device_type": None,
            "device_name": None,
            "ssid": None,
            "wfd": [],
        }
    )
    for path in sys.argv[1:]:
        for frame in read_pcap(path):
            if len(frame) < 24:
                continue
            frame_control = struct.unpack("<H", frame[0:2])[0]
            if (frame_control >> 2) & 0x3 != 0:  # not a management frame
                continue
            subtype = (frame_control >> 4) & 0xF
            if subtype not in (BEACON, PROBE_REQUEST, PROBE_RESPONSE):
                continue
            source = frame[10:16]
            # Fixed parameters differ per subtype.
            if subtype in (BEACON, PROBE_RESPONSE):
                body = frame[36:]
            else:
                body = frame[24:]
            device = devices[mac(source)]
            device["frames"] += 1
            for element_id, payload in parse_ies(body):
                if element_id != 221 or len(payload) < 4:
                    continue
                oui = payload[:3]
                oui_type = payload[3]
                data = payload[4:]
                if oui != WFA_OUI:
                    continue
                if oui_type == P2P_IE_TYPE:
                    attributes = parse_p2p_ie(data)
                    if P2P_ATTR_DEVICE_ADDRESS in attributes:
                        device["p2p_address"] = mac(attributes[P2P_ATTR_DEVICE_ADDRESS])
                    if P2P_ATTR_DEVICE_TYPE in attributes:
                        device["device_type"] = device_type_text(attributes[P2P_ATTR_DEVICE_TYPE])
                    if P2P_ATTR_DEVICE_NAME in attributes:
                        device["device_name"] = attributes[P2P_ATTR_DEVICE_NAME].decode(
                            "utf-8", "replace"
                        )
                    if P2P_ATTR_SSID in attributes:
                        device["ssid"] = attributes[P2P_ATTR_SSID].decode("utf-8", "replace")
                elif oui_type == WFD_IE_TYPE:
                    entry = data.hex()
                    if entry not in device["wfd"]:
                        device["wfd"].append(entry)

    print(f"devices seen in {len(sys.argv) - 1} capture(s)\n")
    for address, device in sorted(devices.items(), key=lambda item: -item[1]["frames"]):
        print(f"  {address}  ({device['frames']} frame(s))")
        if device["p2p_address"]:
            print(f"    p2p device address : {device['p2p_address']}")
        if device["device_type"]:
            print(f"    p2p device type    : {device['device_type']}")
        if device["device_name"]:
            print(f"    device name        : {device['device_name']!r}")
        if device["ssid"]:
            print(f"    ssid               : {device['ssid']!r}")
        if device["wfd"]:
            for entry in device["wfd"]:
                print(f"    WFD element        : {entry}")
        else:
            print("    WFD element        : none")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
