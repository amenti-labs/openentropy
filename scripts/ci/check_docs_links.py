#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DOCS_ROOT = ROOT / "website" / "src" / "content" / "docs"


def iter_markdown_files() -> list[Path]:
    files: list[Path] = []
    files.extend([ROOT / "README.md", ROOT / "CONTRIBUTING.md"])
    files.extend(sorted((ROOT / "docs").glob("*.md")))
    files.extend(sorted(DOCS_ROOT.rglob("*.md")))
    files.extend(sorted(DOCS_ROOT.rglob("*.mdx")))
    return [f for f in files if f.exists()]


def build_site_routes() -> set[str]:
    routes: set[str] = {"/openentropy/"}
    for path in DOCS_ROOT.rglob("*"):
        if path.suffix not in {".md", ".mdx"}:
            continue
        rel = path.relative_to(DOCS_ROOT)
        parts = list(rel.parts)
        stem = parts[-1][: -len(path.suffix)]
        if stem == "index":
            slug = "/".join(parts[:-1])
        else:
            slug = "/".join(parts[:-1] + [stem])
        routes.add(f"/openentropy/{slug + '/' if slug else ''}")
    return routes


def strip_anchor(link: str) -> str:
    return link.split("#", 1)[0]


def main() -> int:
    md_files = iter_markdown_files()
    site_routes = build_site_routes()
    pattern = re.compile(r"\[[^\]]*\]\(([^)]+)\)")

    broken: list[tuple[str, str, str]] = []

    for file_path in md_files:
        text = file_path.read_text(errors="ignore")
        for match in pattern.finditer(text):
            link = match.group(1).strip()
            if not link or link.startswith(("http://", "https://", "mailto:", "#")):
                continue

            raw = strip_anchor(link)

            if raw.startswith("/openentropy/"):
                if raw not in site_routes:
                    broken.append(
                        (str(file_path.relative_to(ROOT)), link, "missing docs route")
                    )
                continue

            if raw.startswith("/"):
                if not (ROOT / raw.lstrip("/")).exists():
                    broken.append(
                        (
                            str(file_path.relative_to(ROOT)),
                            link,
                            "missing absolute path",
                        )
                    )
                continue

            target = (file_path.parent / raw).resolve()
            if not target.exists():
                broken.append(
                    (str(file_path.relative_to(ROOT)), link, "missing relative path")
                )

    if broken:
        print("[docs-link-check] Broken links found:")
        for file_name, link, reason in broken:
            print(f"- {file_name}: {link} ({reason})")
        return 1

    print("[docs-link-check] OK: no broken markdown links detected")
    return 0


if __name__ == "__main__":
    sys.exit(main())
