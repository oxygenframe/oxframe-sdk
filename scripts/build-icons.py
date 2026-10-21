#!/usr/bin/env python3
"""Builds crates/oxframe-icons/data from the npm packages of the icon sets.

    python3 -I scripts/build-icons.py <lucide-static>/package <@tabler/icons>/package <simple-icons>/package

Each set becomes one TSV file, `name<TAB>inner SVG`, sorted by name. Only the
elements inside <svg> are kept (whitespace collapsed, comments, titles and
Tabler's invisible 24x24 frame dropped); the macro wraps them in the set's
own <svg> attributes. The licenses are copied next to the data.
"""

from __future__ import annotations

import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "oxframe-icons" / "data"

INNER = re.compile(r"<svg\b[^>]*>(.*)</svg>", re.S)
FRAME = re.compile(r'<path stroke="none" d="M0 0h24v24H0z" fill="none"\s*/>')


def inner(svg: str) -> str:
    svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S)
    m = INNER.search(svg)
    if not m:
        raise ValueError("no <svg> element")
    body = re.sub(r"<title>.*?</title>", "", m.group(1), flags=re.S)
    body = FRAME.sub("", body)
    body = re.sub(r"\s+", " ", body).strip()
    body = re.sub(r">\s+<", "><", body)
    body = re.sub(r"\s*/>", "/>", body)
    if "\t" in body or "\n" in body:
        raise ValueError("tab or newline left in an icon")
    return body


def write(name: str, files: list[tuple[str, Path]]) -> int:
    rows = sorted((n, inner(p.read_text())) for n, p in files)
    OUT.joinpath(f"{name}.tsv").write_text("".join(f"{n}\t{b}\n" for n, b in rows))
    return len(rows)


def main() -> None:
    lucide, tabler, simple = (Path(a) for a in sys.argv[1:4])
    OUT.mkdir(parents=True, exist_ok=True)
    counts = {
        "lucide": write("lucide", [(p.stem, p) for p in (lucide / "icons").glob("*.svg")]),
        "tabler": write(
            "tabler",
            [(p.stem, p) for p in (tabler / "icons" / "outline").glob("*.svg")]
            + [(p.stem + "-filled", p) for p in (tabler / "icons" / "filled").glob("*.svg")],
        ),
        "simple": write("simple", [(p.stem, p) for p in (simple / "icons").glob("*.svg")]),
    }
    shutil.copy(lucide / "LICENSE", OUT / "LICENSE-lucide")
    shutil.copy(tabler / "LICENSE", OUT / "LICENSE-tabler")
    shutil.copy(simple / "LICENSE.md", OUT / "LICENSE-simple-icons.md")
    for name, n in counts.items():
        size = OUT.joinpath(f"{name}.tsv").stat().st_size
        print(f"{name}: {n} icons, {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
