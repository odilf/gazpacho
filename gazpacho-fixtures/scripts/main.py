#!/usr/bin/env python3
"""Idempotent generation the gazpacho test-video fixtures.

Stored in `target/gazpacho-fixtures/`."""

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import NamedTuple

import chromium
import derived
import realistic
import synthetic
from common import FIXTURES_DIR, REPO_ROOT, Category, Json

MANIFEST_FILE = FIXTURES_DIR / "manifest.json"

CATEGORIES = [Category.SYNTHETIC, Category.DERIVED, Category.CHROMIUM, Category.REALISTIC]



def generation_sources() -> list[Path]:
    return sorted(Path(__file__).parent.glob("*.py"))


def generation_hash() -> str:
    digest = hashlib.sha256()
    for path in generation_sources():
        digest.update(str(path.relative_to(REPO_ROOT)).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


class Manifest(NamedTuple):
    generation_hash: None | str
    videos: dict[Category, list[dict[str, Json]]]


def load_manifest() -> Manifest:
    if MANIFEST_FILE.exists():
        try:
            data = json.loads(MANIFEST_FILE.read_text())
            return Manifest(
                generation_hash=data.get("generation_hash"),
                videos=data.get("videos", {}),
            )
        except json.JSONDecodeError:
            pass
    return Manifest(generation_hash=None, videos={})


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
        "--force",
        action="store_true",
        help="rebuild the generated fixtures and re-download/re-tag the Chromium corpus",
    )
    parser.add_argument(
        "--no-regen-stale",
        action="store_true",
        help="don't regenerate the files if the code is stale",
    )
    args = parser.parse_args()

    categories: set[Category] = set()
    for cateogry in args.categories:
        if cateogry == "all":
            categories.update(CATEGORIES)
        else:
            categories.add(Category(cateogry))

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    (FIXTURES_DIR / "tmp").mkdir(exist_ok=True)
    for cateogry in categories:
        (FIXTURES_DIR / cateogry).mkdir(exist_ok=True)

    manifest = load_manifest()
    gen_hash = "2026-09-08"
    stale_code = manifest.generation_hash != gen_hash
    overwrite = args.force or stale_code

    videos: dict[Category, dict[str, dict[str, Json]]] = {
        category: {str(v["name"]): v for v in vs}
        for category, vs in manifest.videos.items()
    }

    if overwrite:
        if stale_code:
            print(f"current hash is {gen_hash}, found {manifest.generation_hash}")
            if args.no_regen_stale:
                sys.exit(0)

        print("WARNING: overwriting")

        for cateogry in categories:
            videos[cateogry] = {}

    count = 0
    for cateogry, generated in [
        (Category.SYNTHETIC, synthetic.generate(synthetic.all_specs(), overwrite=overwrite)),
        (Category.DERIVED, derived.generate(overwrite)),
        (
            Category.CHROMIUM,
            chromium.generate(force_retag=overwrite, force_download=args.force),
        ),
        (
            Category.REALISTIC,
            realistic.generate(force_retag=overwrite, force_download=args.force),
        ),
    ]:
        if cateogry in categories:
            for video in generated:
                videos[cateogry][video.name] = video.to_json()
                if not video.failed:
                    count += 1

    manifest = manifest._replace(
        generation_hash=gen_hash,
        videos={category: list(vs.values()) for (category, vs) in videos.items()},
    )

    write_manifest(manifest)

    print(f"fixtures ready in {FIXTURES_DIR}: {count} files")


if __name__ == "__main__":
    main()
