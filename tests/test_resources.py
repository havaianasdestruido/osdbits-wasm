"""Synthetic fixtures only: no console firmware, textures, fonts or audio."""

import importlib.util
import random
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import osd_resources as resources  # noqa: E402


def literal(data):
    result = bytearray(struct.pack("<I", len(data)))
    for start in range(0, len(data), 30):
        result.extend(b"\0" * 4)
        result.extend(data[start : start + 30])
    return bytes(result)


def entry(name, size):
    return struct.pack("<10sHI", name, 0, size)


def container(items):
    """A ROMDIR is itself a file, following a 16-byte RESET stub."""
    table_size = (len(items) + 3) * 16
    table = entry(b"RESET", 16) + entry(b"ROMDIR", table_size)
    table += b"".join(entry(name, len(data)) for name, data in items)
    table += bytes(16)
    payload = b"".join(data + bytes((-len(data)) % 16) for _, data in items)
    return bytes(16) + table + payload


class ExpandTests(unittest.TestCase):
    def test_literals_empty_and_descriptor_boundary(self):
        for size in [0, 1, 29, 30, 31, 60, 255]:
            with self.subTest(size=size):
                data = bytes(range(size))
                self.assertEqual(resources.expand(literal(data)), data)

    def test_overlapping_backreferences_all_descriptor_modes(self):
        for mode in range(4):
            # literal A then copy distance=1, length=3 (overlapping copy).
            src = struct.pack("<I", 4) + struct.pack(">I", 0x40000000 | mode)
            self.assertEqual(resources.expand(src + b"A\0\0"), b"AAAA")

    def test_length_and_distance_bit_split_all_modes(self):
        for mode in range(4):
            backref = (2 << (14 - mode)) | 1  # distance 2, length 5
            src = struct.pack("<I", 7) + struct.pack(">I", 0x20000000 | mode)
            self.assertEqual(resources.expand(src + b"AB" + struct.pack(">H", backref)), b"ABABABA")

    def test_rejects_bad_streams(self):
        samples = [
            b"",
            b"\0",
            struct.pack("<I", 1),
            struct.pack("<II", 1, 0),
            struct.pack("<I", 3) + bytes.fromhex("80000000 0000"),
            struct.pack("<I", 4) + bytes.fromhex("40000000 4101"),
            struct.pack("<I", 2) + bytes.fromhex("40000000 410000"),
            struct.pack("<I", 4) + bytes.fromhex("40000000 410001"),
        ]
        for src in samples:
            with self.subTest(src=src.hex()), self.assertRaises(resources.ResourceError):
                resources.expand(src)

    def test_output_limit_checked_before_work(self):
        with self.assertRaisesRegex(resources.ResourceError, "limit"):
            resources.expand(struct.pack("<I", 0xFFFFFFFF))
        self.assertEqual(resources.expand(literal(b"AB"), max_output=2), b"AB")
        with self.assertRaises(resources.ResourceError):
            resources.expand(literal(b"ABC"), max_output=2)
        for limit in [-1, 0, 1.5, True]:
            with self.assertRaises(ValueError):
                resources.expand(literal(b""), max_output=limit)

    def test_seeded_round_trip_and_malformed_campaign(self):
        rng = random.Random(0x05D2026)
        for _ in range(500):
            data = rng.randbytes(rng.randrange(600))
            self.assertEqual(resources.expand(literal(data)), data)
            for cut in {0, 1, 3, len(literal(data)) - 1}:
                if cut < len(literal(data)):
                    with self.assertRaises(resources.ResourceError):
                        resources.expand(literal(data)[:cut])
        for _ in range(2000):
            src = struct.pack("<I", rng.randrange(200)) + rng.randbytes(rng.randrange(100))
            try:
                result = resources.expand(src, max_output=200)
            except resources.ResourceError:
                continue
            self.assertEqual(len(result), struct.unpack_from("<I", src)[0])


class DirectoryTests(unittest.TestCase):
    def test_directory_offsets_and_nested_container(self):
        inner = container([(b"TEXTEST", literal(b"original placeholder"))])
        outer = container([(b"TEXIMAGE", inner)])
        self.assertEqual(resources.find_romdir(inner), 16)
        entries = resources.parse_romdir(inner, 16)
        self.assertEqual(entries["RESET"], (0, 16))
        self.assertEqual(entries["ROMDIR"], (16, 64))
        self.assertEqual(entries["TEXTEST"], (80, len(literal(b"original placeholder"))))
        self.assertEqual(resources.extract_resources(inner), {"TEXTEST": b"original placeholder"})
        self.assertEqual(resources.extract_resources(outer), {"TEXTEST": b"original placeholder"})
        snd = container([(b"SNDIMAGE", inner)])
        self.assertEqual(
            resources.extract_resources(snd, container_name="SNDIMAGE"),
            {"TEXTEST": b"original placeholder"},
        )

    def test_bad_directory_inputs(self):
        valid = container([(b"TEXTEST", literal(b"X"))])
        samples = [
            b"",
            valid[:-16],
            valid[:30],
            container([(b"TEXTEST", b"A"), (b"TEXTEST", b"B")]),
            container([(b"../BAD", b"A")]),
            container([(b"BAD\\NAME", b"A")]),
            container([(b"\xff", b"A")]),
        ]
        for src in samples:
            with self.subTest(src=src[:32].hex()), self.assertRaises(resources.ResourceError):
                resources.parse_romdir(src, 16)
        for base in [-1, 1, len(valid), True]:
            with self.assertRaises(resources.ResourceError):
                resources.parse_romdir(valid, base)

    def test_missing_terminator_and_entry_limits(self):
        with self.assertRaises(resources.ResourceError):
            resources.parse_romdir(entry(b"RESET", 0), 0)
        with self.assertRaisesRegex(resources.ResourceError, "limit"):
            resources.parse_romdir(container([(b"TEXTEST", literal(b"X"))]), 16, max_entries=2)
        with self.assertRaises(resources.ResourceError):
            resources.find_romdir(b"x" + container([]))

    def test_metadata_skipped_and_unsafe_c_symbols_rejected(self):
        self.assertEqual(resources.extract_resources(container([(b"-", b"not compressed")])), {})
        for name in [b"BAD.NAME", b"9BAD", b"BAD-NAME"]:
            with self.assertRaises(resources.ResourceError):
                resources.extract_resources(container([(name, literal(b"A"))]))

    def test_container_spans_and_total_budget(self):
        src = container([(b"ONE", literal(b"ABC")), (b"TWO", literal(b"DEF"))])
        with self.assertRaisesRegex(resources.ResourceError, "limit"):
            resources.extract_resources(src, max_total_output=5)
        self.assertEqual(
            resources.extract_resources(src, max_total_output=6), {"ONE": b"ABC", "TWO": b"DEF"}
        )
        with self.assertRaisesRegex(resources.ResourceError, "limit"):
            resources.extract_resources(src, max_input=8)
        # ROMDIR claims to be a different file span than the actual table.
        broken = bytearray(src)
        struct.pack_into("<I", broken, 44, 16)
        with self.assertRaises(resources.ResourceError):
            resources.parse_romdir(broken, 16)

    def test_seeded_directory_campaign(self):
        rng = random.Random(0xD12026)
        for _ in range(1000):
            src = rng.randbytes(rng.randrange(300))
            try:
                result = resources.parse_romdir(src, 0, max_entries=16)
            except resources.ResourceError:
                continue
            for offset, size in result.values():
                self.assertLessEqual(offset + size, len(src))


class CliTests(unittest.TestCase):
    scripts = [ROOT / "tools/extract-res.py", ROOT / "osdbits/tools/extract-res.py"]

    def run_cli(self, script, *args, cwd=None):
        return subprocess.run(
            [sys.executable, str(script), *map(str, args)],
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def test_both_entry_points_from_external_working_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            image = base / "synthetic.bin"
            image.write_bytes(container([(b"TEXTEST", literal(b"ABC"))]))
            for i, script in enumerate(self.scripts):
                out = base / str(i)
                result = self.run_cli(script, image, out, "--raw", cwd=base)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((out / "TEXTEST_EXP").read_bytes(), b"ABC")
                self.assertEqual(
                    (out / "TEXTEST_EXP.inc").read_text(),
                    "unsigned char TEXTEST_EXP[] = {\n  0x41, 0x42, 0x43\n};\n"
                    "unsigned int TEXTEST_EXP_len = 3;\n",
                )

    def test_invalid_input_never_creates_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            image = base / "synthetic.bin"
            # A valid first resource must not be written before a bad second one fails.
            image.write_bytes(container([(b"GOOD", literal(b"X")), (b"BAD", b"\xff")]))
            for script in self.scripts:
                result = self.run_cli(script, image, base / "output")
                self.assertEqual(result.returncode, 2)
                self.assertIn("error:", result.stderr)
                self.assertNotIn("Traceback", result.stderr)
                self.assertFalse((base / "output").exists())
                self.assertEqual(
                    self.run_cli(script, base / "missing", base / "output").returncode, 2
                )

    def test_help_and_library_compatibility(self):
        for i, script in enumerate(self.scripts):
            self.assertEqual(self.run_cli(script, "--help").returncode, 0)
            spec = importlib.util.spec_from_file_location(f"extract_compat_{i}", script)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(module.expand(literal(b"X")), b"X")

    def test_write_inc_rejects_injection_and_empty_resource(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "empty.inc"
            resources.write_inc(path, "EMPTY", b"")
            self.assertIn("EMPTY_len = 0;", path.read_text())
            with self.assertRaises(resources.ResourceError):
                resources.write_inc(path, "bad;code", b"X")
            with self.assertRaises(resources.ResourceError):
                resources.write_inc(path, "../bad", b"X")


class HistoricalParityTests(unittest.TestCase):
    def test_original_c_decoder_on_valid_synthetic_streams(self):
        # Do not fuzz the unsafe historical C decoder. Only known-valid streams.
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            binary = base / "ps2expand"
            subprocess.run(
                [
                    "cc",
                    "-std=c99",
                    "-Wall",
                    "-Wextra",
                    "-O2",
                    str(ROOT / "ps2expand.c"),
                    "-o",
                    str(binary),
                ],
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            )
            rng = random.Random(0xC2026)
            streams = [literal(rng.randbytes(n)) for n in [0, 1, 29, 30, 31, 100, 1024]]
            for mode in range(4):
                streams.append(
                    struct.pack("<I", 7)
                    + struct.pack(">I", 0x20000000 | mode)
                    + b"AB"
                    + struct.pack(">H", (2 << (14 - mode)) | 1)
                )
            for src in streams:
                image = base / "valid.lz"
                image.write_bytes(src)
                actual = subprocess.run(
                    [str(binary), str(image)], capture_output=True, check=True, timeout=10
                ).stdout
                self.assertEqual(actual, resources.expand(src))

    def test_advanced_table_functions_are_preserved(self):
        spec = importlib.util.spec_from_file_location("advanced_extract", CliTests.scripts[1])
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        # Synthetic module image, containing no actual console glyphs or meshes.
        img = bytearray(0xA0000)

        def put(va, data):
            start = va - module.OSDSYS_LOAD
            img[start : start + len(data)] = data

        put(module.FONT_METRICS_VA, struct.pack("<194I", *([1, 12] * 97)))
        put(module.OSD_METRICS_VA, struct.pack("<70I", *([2, 13] * 35)))
        put(module.FONT_CLUT_VA, struct.pack("<16I", *range(16)))
        put(module.STRINGS_VA, struct.pack("<299I", module.OSDSYS_LOAD + 0x90000, *([0] * 298)))
        put(module.OSDSYS_LOAD + 0x90000, b'original "synthetic" \\ \x07\0')
        for scene in [module.SCENE_ROD, module.SCENE_CUBE]:
            put(scene + 4, struct.pack("<4I", 1, 0x280000, 0x280040, 0x280050))
        put(0x280000, struct.pack("<16f", *([1.0, 2.0, 3.0, 1.0] * 4)))
        put(0x280040, struct.pack("<4f", 0.0, 1.0, 0.0, 0.0))
        put(0x280050, struct.pack("<16f", *([0.0, 0.0, 0.0, 1.0] * 4)))
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            font, mesh = base / "FONTDATA.inc", base / "MENUGEOM.inc"
            module.write_font_tables(font, img)
            module.write_menu_geometry(mesh, img)
            self.assertIn("fontAsciiMetrics[97][2]", font.read_text())
            self.assertIn("fontOsdMetrics[35][2]", font.read_text())
            self.assertIn('original \\"synthetic\\"', font.read_text())
            self.assertIn("menuRodFaces = 1;", mesh.read_text())
            self.assertIn("menuCubeFaces = 1;", mesh.read_text())
            self.assertIn("menuCubePos[5][4]", mesh.read_text())
            unit = base / "synthetic.c"
            unit.write_text(font.read_text() + mesh.read_text())
            subprocess.run(
                ["cc", "-std=c99", "-Wall", "-Wextra", "-Werror", "-fsyntax-only", str(unit)],
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            )
            image = base / "synthetic.bin"
            image.write_bytes(container([(b"OSDSYS", literal(img))]))
            out = base / "tables"
            result = CliTests().run_cli(CliTests.scripts[1], image, out, "--tables")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((out / "FONTDATA.inc").read_bytes(), font.read_bytes())
            self.assertEqual((out / "MENUGEOM.inc").read_bytes(), mesh.read_bytes())

    def test_atomic_output_failure_preserves_previous_file(self):
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "existing.inc"
            path.write_bytes(b"previous")
            with patch.object(
                resources.os, "replace", side_effect=OSError("synthetic disk failure")
            ):
                with self.assertRaises(OSError):
                    resources.write_inc(path, "SAFE", b"X")
            self.assertEqual(path.read_bytes(), b"previous")
            self.assertEqual(list(Path(tmp).iterdir()), [path])


if __name__ == "__main__":
    unittest.main()
