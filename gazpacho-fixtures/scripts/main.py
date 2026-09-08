#!/usr/bin/env python3
"""Idempotent generation the gazpacho test-video fixtures.

Stored in `target/gazpacho-fixtures/`."""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import NamedTuple

import chromium
import derived
import realistic
import synthetic
from common import FIXTURES_DIR, REPO_ROOT, Category, Json

MANIFEST_FILE = FIXTURES_DIR / "manifest.json"

CATEGORIES = [
    Category.SYNTHETIC,
    Category.DERIVED,
    Category.CHROMIUM,
    Category.REALISTIC,
]


def generation_sources() -> list[Path]:
    return sorted(Path(__file__).parent.glob("*.py"))


def generation_hash() -> str:
    digest = hashlib.sha256()
    for path in generation_sources():
        digest.update(str(path.relative_to(REPO_ROOT)).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


class Manifest(NamedTuple):
    videos: dict[Category, list[dict[str, Json]]]


def load_manifest() -> tuple[Manifest, bool]:
    if MANIFEST_FILE.exists():
        try:
            data = json.loads(MANIFEST_FILE.read_text())
            return Manifest(videos=data.get("videos", {})), True
        except json.JSONDecodeError:
            pass
    return Manifest(videos={cat: [] for cat in CATEGORIES}), False


def write_manifest(manifest: Manifest) -> None:
    # Temp-plus-rename so a concurrently running test binary never observes a
    # half-written manifest.
    tmp = MANIFEST_FILE.with_name(f".manifest-{os.getpid()}")
    tmp.write_text(json.dumps(manifest._asdict(), indent=2, sort_keys=True) + "\n")
    tmp.rename(MANIFEST_FILE)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate gazpacho test-video fixtures"
    )
    parser.add_argument(
        "categories",
        nargs="*",
        default=["all"],
        choices=["all"] + [c.value for c in CATEGORIES],
        help="which categories to generate (default: all)",
    )
    parser.add_argument(
        "--regenerate",
        action="store_true",
        help=(
            "re-encode the synthetic and derived fixtures even if they already "
            "exist (does not touch the downloaded corpora)"
        ),
    )
    parser.add_argument(
        "--retag",
        action="store_true",
        help=(
            "re-run the hashing/decode annotation pass on the chromium and "
            "realistic corpora even if a cached annotation manifest exists "
            "(does not re-download them)"
        ),
    )
    args = parser.parse_args()

    categories: set[Category] = set()
    for category in args.categories:
        if category == "all":
            categories.update(CATEGORIES)
        else:
            categories.add(Category(category))

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    (FIXTURES_DIR / "tmp").mkdir(exist_ok=True)
    for cateogry in categories:
        (FIXTURES_DIR / cateogry).mkdir(exist_ok=True)

    manifest, exists = load_manifest()
    videos: dict[Category, dict[str, dict[str, Json]]] = {
        category: {str(v["name"]): v for v in vs}
        for category, vs in manifest.videos.items()
    }

    # If manifest already exists, just exit.
    if exists and not (args.regenerate or args.retag):
        return

    # A forced rebuild starts from an empty registry for the affected
    # categories so entries whose specs/clips no longer exist don't linger.
    if args.regenerate:
        for cateogry in categories & {Category.SYNTHETIC, Category.DERIVED}:
            videos[cateogry] = {}
    if args.retag:
        for cateogry in categories & {Category.CHROMIUM, Category.REALISTIC}:
            videos[cateogry] = {}

    count = 0
    for category, generated in [
        (
            Category.SYNTHETIC,
            synthetic.generate(synthetic.all_specs(), args.regenerate),
        ),
        (Category.DERIVED, derived.generate(args.regenerate)),
        (Category.CHROMIUM, chromium.generate(retag=args.regenerate)),
        (Category.REALISTIC, realistic.generate()),
    ]:
        if category not in categories:
            continue
        for video in generated:
            videos[category][video.name] = video.to_json()
            if not video.failed:
                count += 1


    manifest = manifest._replace(
        videos={category: list(vs.values()) for (category, vs) in videos.items()},
    )

    write_manifest(manifest)

    print(f"fixtures ready in {FIXTURES_DIR}: {count} files")


if __name__ == "__main__":
    main()
