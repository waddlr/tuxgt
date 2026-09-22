"""Prefix-fallback vectors for tuxgt_apply.correlate (hook side).

Exact (prefix, exe) hits keep priority; an exe with no key of its own
resolves when its prefix section names exactly one rel (launcher-first
chains: MO2 outer, SKSE middle — the inner game exe inherits the env).
Contested prefixes, unknown prefixes, and missing conf files miss.
Run: `python3 test_correlate.py` (stdlib unittest only). Not shipped:
both install paths (Makefile, userland/install.rs) name hook files
explicitly.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from tuxgt_apply import correlate


def env(prefix: str, exe: str) -> dict[str, str]:
    return {"STEAM_COMPAT_DATA_PATH": prefix, "EXE": exe}


class CorrelateTest(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.data = Path(tmp.name)
        (self.data / "games" / "steam" / "mo2test").mkdir(parents=True)
        (self.data / "games" / "steam" / "mo2test" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        (self.data / "games" / "load-correlator.ini").write_text(
            "# tuxgt load-correlator\n"
            "\n"
            "[/pfx/mo2]\n"
            "c:/games/stock game/skyrimse.exe=steam/mo2test\n"
        )

    def test_exact_hit(self) -> None:
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\Stock Game\\SkyrimSE.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))

    def test_fallback_hit_single_rel(self) -> None:
        # SKSE middle link: no key of its own, single-rel prefix resolves.
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))

    def test_exact_beats_fallback(self) -> None:
        # Contested prefix, but the exe has its own key: exact wins.
        (self.data / "games" / "heroic_gog" / "abc").mkdir(parents=True)
        (self.data / "games" / "heroic_gog" / "abc" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/tool/tool.exe=heroic_gog/abc\n")
        conf = correlate(self.data, env("/pfx/mo2", "c:/games/tool/tool.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("heroic_gog/abc/tux-protonfixes.conf"))

    def test_contested_prefix_misses(self) -> None:
        (self.data / "games" / "heroic_gog" / "abc").mkdir(parents=True)
        (self.data / "games" / "heroic_gog" / "abc" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/tool/tool.exe=heroic_gog/abc\n")
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe")))

    def test_unknown_prefix_misses(self) -> None:
        self.assertIsNone(correlate(self.data, env("/pfx/unknown", "C:\\Games\\skse64_loader.exe")))

    def test_missing_conf_misses(self) -> None:
        # Rel resolves but its session file is gone: miss, not a bad path.
        (self.data / "games" / "steam" / "mo2test" / "tux-protonfixes.conf").unlink()
        self.assertIsNone(
            correlate(self.data, env("/pfx/mo2", "C:\\Games\\Stock Game\\SkyrimSE.exe"))
        )
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe")))

    def test_missing_keys_miss(self) -> None:
        self.assertIsNone(correlate(self.data, env("", "C:\\Games\\skse64_loader.exe")))
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "")))



    def test_empty_value_ignored(self) -> None:
        # Stray empty-valued key: ignored, single real rel still resolves.
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/stray/stray.exe=\n")
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))
if __name__ == "__main__":
    unittest.main(verbosity=2)
