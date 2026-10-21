#!/usr/bin/env python3
"""Compiles every scene in skills/*/SKILL.md against this checkout, so the
skills agents install never teach an API that no longer exists. Part of
release-check."""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        crate = Path(tmp)
        (crate / "src").mkdir()
        (crate / "Cargo.toml").write_text(
            "[package]\nname = \"skill-snippets\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n"
            f"[dependencies]\noxframe = {{ path = \"{ROOT / 'crates' / 'oxframe'}\", features = [\"3d\", \"icons\", \"video\"] }}\n\n[workspace]\n"
        )
        mods = []
        for path in sorted((ROOT / "skills").glob("*/SKILL.md")):
            blocks = re.findall(r"```rust\n(.*?)```", path.read_text(), re.S)
            for i, block in enumerate(blocks):
                if "#[oxframe::scene]" not in block:
                    continue
                name = f"{path.parent.name.replace('-', '_')}_{i}"
                (crate / "src" / f"{name}.rs").write_text(
                    "#![allow(dead_code, unused)]\nuse oxframe::prelude::*;\n\n" + block
                )
                mods.append(name)
        (crate / "src" / "main.rs").write_text("".join(f"mod {m};\n" for m in mods) + "\nfn main() {}\n")
        result = subprocess.run(
            ["cargo", "check", "--quiet"],
            cwd=crate,
            env={**__import__("os").environ, "CARGO_TARGET_DIR": str(ROOT / "target" / "skill-snippets")},
        )
        if result.returncode != 0:
            sys.exit("check-skills: a scene in skills/ does not compile (see above)")
        print(f"check-skills: {len(mods)} scenes in skills/ compile")


if __name__ == "__main__":
    main()
