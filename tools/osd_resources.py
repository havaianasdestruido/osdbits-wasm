"""Bounded local OSDSYS resource extraction, derived from extract-res.py.

Only use with resources you are authorized to access. This module does not
supply firmware or grant redistribution rights. Trailing compressed padding is
permitted; truncated streams, output overshoot and invalid references are not.
"""

import argparse
import os
import re
import struct
import tempfile
from pathlib import Path

MAX_INPUT = 64 * 1024 * 1024
MAX_OUTPUT = 32 * 1024 * 1024
MAX_TOTAL_OUTPUT = 128 * 1024 * 1024
MAX_ENTRIES = 4096
SAFE_NAME = re.compile(r"[A-Za-z0-9_.-]{1,10}\Z")
C_SYMBOL = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
METADATA = {"RESET", "ROMDIR", "EXTINFO"}


class ResourceError(ValueError):
    """A malformed or over-budget resource, safe to show as a local error."""


def _limit(value):
    if type(value) is not int or value <= 0:
        raise ValueError("limits must be positive integers")


def parse_romdir(data, base, *, max_entries=MAX_ENTRIES):
    """Read a terminated ROMDIR; validate names, spans, duplicates and bounds.

    File offsets are relative to the supplied image, not to the table. The
    table itself is normally a file at a nonzero offset in that image.
    """
    _limit(max_entries)
    if type(base) is not int or base < 0 or base % 16 or base + 16 > len(data):
        raise ResourceError("invalid ROMDIR offset or alignment")
    entries = {}
    offset = 0
    pos = base
    for _ in range(max_entries + 1):
        if pos + 16 > len(data):
            raise ResourceError("truncated or unterminated ROMDIR")
        raw_name = bytes(data[pos : pos + 10])
        pos += 16
        if raw_name == bytes(10):
            if "ROMDIR" in entries:
                table_offset, table_size = entries["ROMDIR"]
                if table_offset != base or table_size < pos - base:
                    raise ResourceError("ROMDIR self span does not cover its table")
            return entries
        if len(entries) >= max_entries:
            raise ResourceError("ROMDIR entry limit exceeded")
        try:
            name = raw_name.rstrip(b"\0").decode("ascii")
        except UnicodeDecodeError as exc:
            raise ResourceError("ROMDIR name is not ASCII") from exc
        if not SAFE_NAME.fullmatch(name) or name in {".", ".."}:
            raise ResourceError("unsafe ROMDIR filename")
        if name in entries:
            raise ResourceError(f"duplicate ROMDIR name: {name}")
        _, size = struct.unpack_from("<HI", data, pos - 6)
        if offset + size > len(data):
            raise ResourceError(f"ROMDIR file span exceeds input: {name}")
        entries[name] = (offset, size)
        offset += (size + 15) & ~15
    raise ResourceError("ROMDIR entry limit exceeded")


def find_romdir(data):
    """Find an aligned RESET entry followed by ROMDIR, as in the original tool."""
    pos = 0
    while True:
        pos = data.find(b"RESET\0\0\0\0\0", pos)
        if pos < 0:
            raise ResourceError("no aligned ROMDIR found (not a BIOS/container image?)")
        if pos % 16 == 0 and data[pos + 16 : pos + 26] == b"ROMDIR\0\0\0\0":
            return pos
        pos += 1


def expand(src, *, max_output=MAX_OUTPUT):
    """Decode original 30-flag LZ blocks with strict input/output budgets.

    Header size is little-endian, descriptors and backreferences big-endian.
    Overlapping copies are intentional and preserve the original algorithm.
    """
    _limit(max_output)
    if len(src) < 4:
        raise ResourceError("truncated expanded-size header")
    size = struct.unpack_from("<I", src)[0]
    if size > max_output:
        raise ResourceError("expanded resource exceeds output limit")
    p = 4
    dst = bytearray()
    ndesc = desc = shift = mask = 0
    while len(dst) < size:
        if ndesc == 0:
            if p + 4 > len(src):
                raise ResourceError("truncated LZ descriptor")
            desc = struct.unpack_from(">I", src, p)[0]
            p += 4
            mode = desc & 3
            shift = 14 - mode
            mask = 0x3FFF >> mode
            ndesc = 30
        if p >= len(src):
            raise ResourceError("truncated LZ literal or backreference")
        b = src[p]
        p += 1
        if desc & 0x80000000:
            if p >= len(src):
                raise ResourceError("truncated LZ backreference")
            h = b << 8 | src[p]
            p += 1
            back = 1 + (h & mask)
            length = 3 + (h >> shift)
            if back > len(dst):
                raise ResourceError("LZ backreference precedes output")
            if len(dst) + length > size:
                raise ResourceError("LZ backreference exceeds declared output size")
            for _ in range(length):
                dst.append(dst[-back])
        else:
            dst.append(b)
        desc = (desc << 1) & 0xFFFFFFFF
        ndesc -= 1
    return bytes(dst)


def extract_resources(
    data,
    *,
    container_name="TEXIMAGE",
    max_input=MAX_INPUT,
    max_output=MAX_OUTPUT,
    max_total_output=MAX_TOTAL_OUTPUT,
):
    """Validate an image completely before returning any named output bytes."""
    for limit in (max_input, max_output, max_total_output):
        _limit(limit)
    if len(data) > max_input:
        raise ResourceError("image exceeds input limit")
    if not SAFE_NAME.fullmatch(container_name):
        raise ResourceError("invalid container name")
    entries = parse_romdir(data, find_romdir(data))
    if container_name in entries:
        off, size = entries[container_name]
        data = data[off : off + size]
        entries = parse_romdir(data, find_romdir(data))
    outputs = {}
    total = 0
    for name, (off, size) in entries.items():
        if name in METADATA or name.startswith("-"):
            continue
        if not C_SYMBOL.fullmatch(name):
            raise ResourceError(f"resource name is not a safe C identifier: {name}")
        remaining = max_total_output - total
        if remaining <= 0:
            # A zero-length resource may still fit at the exact total budget.
            if size < 4 or struct.unpack_from("<I", data, off)[0] != 0:
                raise ResourceError("resources exceed total output limit")
        expanded = expand(data[off : off + size], max_output=min(max_output, max(1, remaining)))
        outputs[name] = expanded
        total += len(expanded)
    return outputs


def _atomic_write(path, data):
    """Replace a single output without truncating existing files on write failure.

    This is per-file atomicity, not an all-files transaction. An I/O error can
    leave a mix of old/new files; CLI validation errors create no outputs.
    """
    path = Path(path)
    tmp = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".osdbits-", delete=False) as f:
            tmp = Path(f.name)
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    finally:
        if tmp is not None:
            tmp.unlink(missing_ok=True)


def write_inc(path, symbol, data):
    if not C_SYMBOL.fullmatch(symbol):
        raise ResourceError("unsafe output C identifier")
    lines = [f"unsigned char {symbol}[] = {{\n"]
    for i in range(0, len(data), 12):
        row = data[i : i + 12]
        lines.append("  " + ", ".join(f"0x{b:02x}" for b in row))
        lines.append(",\n" if i + 12 < len(data) else "\n")
    lines.extend(["};\n", f"unsigned int {symbol}_len = {len(data)};\n"])
    _atomic_write(path, "".join(lines).encode("ascii"))


def main(argv=None):
    parser = argparse.ArgumentParser(description="Extract authorized local OSDSYS resources")
    parser.add_argument("image", help="user-supplied BIOS or resource container")
    parser.add_argument("outdir", help="output directory (normally osdbits/res)")
    parser.add_argument("--container", default="TEXIMAGE", help="container (default TEXIMAGE)")
    parser.add_argument("--raw", action="store_true", help="also write raw expanded resources")
    args = parser.parse_args(argv)
    try:
        # Bounded read even if the file grows after stat or is a special file.
        with Path(args.image).open("rb") as f:
            data = f.read(MAX_INPUT + 1)
        outputs = extract_resources(data, container_name=args.container)
        outdir = Path(args.outdir)
        outdir.mkdir(parents=True, exist_ok=True)
        for name, expanded in outputs.items():
            symbol = name + "_EXP"
            write_inc(outdir / (symbol + ".inc"), symbol, expanded)
            if args.raw:
                _atomic_write(outdir / symbol, expanded)
            print(f"{name:<10} -> {len(expanded):6d}")
        print(f"{len(outputs)} resources extracted to {outdir}")
    except (ResourceError, OSError) as exc:
        parser.exit(2, f"error: {exc}\n")
    return 0
