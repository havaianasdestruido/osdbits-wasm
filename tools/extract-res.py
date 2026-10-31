#!/usr/bin/env python3
# extract-res.py - regenerate osdbits/res/ from a PS2 BIOS dump.
#
# The repo does not ship Sony's texture data; this script extracts it
# from a BIOS image you dumped from your own console:
#
#     python3 tools/extract-res.py path/to/bios.bin osdbits/res
#
# It locates the ROMDIR (name[10], u16 extsize, u32 size entries, data
# packed in order, 16-byte aligned), finds the TEXIMAGE container (which
# is itself a nested ROMDIR of compressed subfiles), decompresses each
# subfile (the OSDSYS "expand" LZ scheme) and writes xxd -i style .inc
# files for res.c, plus the raw expanded data with --raw.
#
# A bare TEXIMAGE file (already extracted from a BIOS) also works as
# input.

# Shared, bounded parser; public helpers retained for existing local scripts.
from osd_resources import expand, find_romdir, main, parse_romdir, write_inc  # noqa: F401

if __name__ == "__main__":
    raise SystemExit(main())
