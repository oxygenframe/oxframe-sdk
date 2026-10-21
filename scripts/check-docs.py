#!/usr/bin/env python3
"""The site docs must describe this SDK version before it is tagged.

Reads the changelog section for the workspace version and requires every
public API named there to appear in the current docs (not in an archived
snapshot). A name counts when the changelog puts it in backticks and it is a
type (`Custom`), a path (`Custom::new`), a flag (`inspect --graph`), or a
call (`.toward_camera()`).

Also requires a live player on the Going further pages that teach by showing
a picture: components, nodes, effects, camera, timing, rendering.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT.parent / "oxframe-web"
DOCS = WEB / "apps" / "site" / "src" / "routes" / "docs"
PLAYERS = (
    "guide/components/+page.svelte",
    "nodes/+page.svelte",
    "effects/+page.svelte",
    "camera/+page.svelte",
    "charts/+page.svelte",
    "custom-nodes/+page.svelte",
    "timing/+page.svelte",
    "rendering/+page.svelte",
)


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    match = re.search(r'^version = "([^"]+)"', text, re.M)
    if not match:
        raise SystemExit("check-docs: Cargo.toml has no version")
    return match.group(1)


def changelog_section(ver: str) -> str:
    lines = (ROOT / "CHANGELOG.md").read_text().splitlines()
    start = re.compile(rf"^## {re.escape(ver)}( |$)")
    out: list[str] = []
    on = False
    for line in lines:
        if start.match(line):
            on = True
            continue
        if on and line.startswith("## "):
            break
        if on:
            out.append(line)
    if not out:
        raise SystemExit(f"check-docs: CHANGELOG.md has no section for {ver}")
    return "\n".join(out)


def feature_names(section: str) -> list[str]:
    names: list[str] = []
    for span in re.findall(r"`([^`]+)`", section):
        span = span.strip()
        if not span or span in names:
            continue
        if "::" in span or "--" in span or span.startswith(".") or "(" in span:
            names.append(span)
        elif re.fullmatch(r"[A-Z][A-Za-z0-9]+", span):
            names.append(span)
    return names


def live_docs() -> str:
    parts: list[str] = []
    for path in DOCS.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(DOCS)
        if relative.parts and re.fullmatch(r"\d+\.\d+(\.\d+)?", relative.parts[0]):
            continue
        if path.suffix not in {".svelte", ".ts", ".md"}:
            continue
        parts.append(path.read_text(errors="replace"))
    return "\n".join(parts)


def main() -> None:
    if not DOCS.is_dir():
        raise SystemExit(f"check-docs: expected the site docs at {DOCS}")
    ver = version()
    names = feature_names(changelog_section(ver))
    docs = live_docs()
    missing = [name for name in names if name not in docs]
    if missing:
        joined = "\n  ".join(missing)
        raise SystemExit(
            f"check-docs: oxframe {ver} names these in CHANGELOG.md, and the current\n"
            f"site docs do not mention them. Update the docs (Going further or the\n"
            f"guide) before tagging, then rerun release-check.\n  {joined}"
        )
    absent = [page for page in PLAYERS if "LiveExample" not in (DOCS / page).read_text()]
    if absent:
        joined = "\n  ".join(absent)
        raise SystemExit(
            "check-docs: these Going further pages show a feature without a player.\n"
            "Add <LiveExample>, the same piece the guide uses, before tagging.\n"
            f"  {joined}"
        )
    shown = ", ".join(names) if names else "(none — changelog names no public API)"
    print(f"check-docs: {ver} docs mention {shown}")


if __name__ == "__main__":
    main()
