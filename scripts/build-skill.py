#!/usr/bin/env python3
"""The `oxframe` agent skill at skills/oxframe/, for `npx skills add
oxygenframe/oxframe-sdk` (skills.sh) and other skill catalogs.

Built from the skill that `oxframe init` writes into a project (the
templates in the sibling oxframe-cli checkout), for this SDK version, with a
first section on getting a project when there is none. Run it in every
release; `--check` fails when skills/oxframe/ is out of date.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT.parent / "oxframe-cli"
OUT = ROOT / "skills" / "oxframe"
# The Claude Code plugin (plugins/oxframe) carries the same skill.
PLUGIN = ROOT / "plugins" / "oxframe"

START = """
## No project yet

When the user wants a video and there is no oxframe project (no `oxframe.toml`):

1. Install the CLI: `curl -fsSL https://github.com/oxygenframe/oxframe-cli/releases/latest/download/install.sh | sh`
   (or `cargo install oxframe-cli`). Rendering needs ffmpeg.
2. `oxframe init my-video` creates the project: `src/main.rs` with one scene,
   `oxframe.toml`, and this skill with project notes in `AGENTS.md`.
3. `cd my-video`, write the scenes, then follow the workflow below. `oxframe render`
   writes `out/my-video.mp4`.

Rust is needed on the machine, but the user does not need to know it: the
scenes are short async functions, and `oxframe check` tells you what is wrong.
"""

DESCRIPTION = (
    "Make videos from code with oxframe: explainers, code walkthroughs, charts, "
    "matrices, 3D, slides and release videos as Rust scenes, checked with "
    "`oxframe check` (storyboard and report) and rendered to MP4 without a browser. "
    "Use when the user asks for an animation, a motion graphic, a video from code, "
    "or changes to an oxframe project."
)


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    match = re.search(r'^version = "([^"]+)"', text, re.M)
    if not match:
        raise SystemExit("build-skill: Cargo.toml has no version")
    return match.group(1)


def cli_version() -> str:
    text = (CLI / "Cargo.toml").read_text()
    match = re.search(r'^version = "([^"]+)"', text, re.M)
    if not match:
        raise SystemExit("build-skill: oxframe-cli/Cargo.toml has no version")
    return match.group(1)


def fill(template: str, sdk: str) -> str:
    series = ".".join(sdk.split(".")[:2])
    return (
        template.replace("{{VERSION}}", cli_version())
        .replace("{{SDK_VERSION}}", sdk)
        .replace("{{DOCS_SERIES}}", series)
    )


def build() -> dict[str, str]:
    sdk = version()
    templates = CLI / "templates" / "agents"
    skill = fill((templates / "SKILL.md").read_text(), sdk)
    # A catalog-wide description, and the getting-started section first.
    # Quoted: the description has ": " in it, which plain YAML would misread.
    quoted = '"' + DESCRIPTION.replace("\\", "\\\\").replace('"', '\\"') + '"'
    skill = re.sub(r"^description: .*$", lambda _: f"description: {quoted}", skill, count=1, flags=re.M)
    head, sep, rest = skill.partition("\n## ")
    skill = head + "\n" + START.strip() + "\n" + sep + rest
    skill = skill.replace("in this project.", "in an oxframe project.")
    return {
        "SKILL.md": skill,
        "reference.md": fill((templates / "reference.md").read_text(), sdk),
    }


def frontmatter(skill: str) -> dict:
    import yaml

    _, front, _ = skill.split("---", 2)
    return yaml.safe_load(front)


def main() -> None:
    files = build()
    meta = frontmatter(files["SKILL.md"])
    if meta.get("name") != "oxframe" or meta.get("description") != DESCRIPTION:
        raise SystemExit("build-skill: SKILL.md frontmatter does not parse back")
    # The Claude Code, Cursor and Codex manifests carry the SDK version.
    manifests = [PLUGIN / f".{agent}-plugin" / "plugin.json" for agent in ("claude", "cursor", "codex")]
    targets = {OUT / n: t for n, t in files.items()}
    targets |= {PLUGIN / "skills" / "oxframe" / n: t for n, t in files.items()}
    # The task skills next to it (written by hand) go into the plugin as they are.
    for other in sorted((ROOT / "skills").glob("*/SKILL.md")):
        if other.parent.name == "oxframe":
            continue
        text = other.read_text()
        meta = frontmatter(text)
        if meta.get("name") != other.parent.name or not meta.get("description"):
            raise SystemExit(f"build-skill: {other.relative_to(ROOT)} needs name and description")
        targets[PLUGIN / "skills" / other.parent.name / "SKILL.md"] = text
    for manifest in manifests:
        targets[manifest] = re.sub(r'"version": "[^"]*"', f'"version": "{version()}"', manifest.read_text())
    if "--check" in sys.argv:
        stale = [str(p.relative_to(ROOT)) for p, text in targets.items() if not p.exists() or p.read_text() != text]
        if stale:
            raise SystemExit(
                f"build-skill: {', '.join(stale)} out of date; run scripts/build-skill.py"
            )
        print("build-skill: skills/oxframe is up to date")
        return
    for path, text in targets.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    print(f"build-skill: wrote skills/oxframe for oxframe {version()}")


if __name__ == "__main__":
    main()
