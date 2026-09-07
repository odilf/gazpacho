#!/usr/bin/env python3
"""Generate the gazpacho test-video corpus under `target/gazpacho-fixtures/`.

Idempotent: regenerates only when these generation scripts changed (hashed) or
when `--force` is passed, then writes `manifest.json` describing every
synthetic/derived/chromium video for the Rust test crate to consume. The
manifest is grouped by kind — `synthetic` / `derived` / `chromium` — so each
kind deserializes into its own Rust type.

usage: python3 gazpacho-fixtures/scripts/generate.py [--force] [kind…]
kinds: synthetic | derived | chromium | all   (default: all)
--force rebuilds the generated fixtures for synthetic/derived, re-downloads the
cached Chromium corpus, and re-tags it.
"""

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import NamedTuple

import chromium
import derived
import synthetic
from common import FIXTURES_DIR, REPO_ROOT, Json

MANIFEST_FILE = FIXTURES_DIR / "manifest.json"

KIND_ORDER = ["synthetic", "derived", "chromium"]


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
    videos: dict[str, list[dict[str, Json]]]


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
    tmp.write_text(json.dumps(manifest._asdict(), indent=2) + "\n")
    tmp.rename(MANIFEST_FILE)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate gazpacho test-video fixtures"
    )
    parser.add_argument(
        "kinds",
        nargs="*",
        default=["all"],
        choices=["all"] + KIND_ORDER,
        help="which kinds to generate (default: all)",
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

    kinds: set[str] = set()
    for kind in args.kinds:
        if kind == "all":
            kinds.update(KIND_ORDER)
        else:
            kinds.add(kind)

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    (FIXTURES_DIR / "tmp").mkdir(exist_ok=True)
    for kind in kinds:
        (FIXTURES_DIR / kind).mkdir(exist_ok=True)

    manifest = load_manifest()
    gen_hash = generation_hash()
    stale_code = manifest.generation_hash != gen_hash
    overwrite = args.force or stale_code

    videos: dict[str, dict[str, dict[str, Json]]] = {
        category: {str(v["name"]): v for v in vs}
        for category, vs in manifest.videos.items()
    }

    if overwrite:
        if stale_code:
            print(f"current hash is {gen_hash}, found {manifest.generation_hash}")
            if args.no_regen_stale:
                sys.exit(0)

        print("WARNING: overwriting")

        for kind in kinds:
            videos[kind] = {}

    count = 0
    for kind, generated in [
        ("synthetic", synthetic.generate(synthetic.all_specs(), overwrite)),
        ("derived", derived.generate(overwrite)),
        # Code changes re-tag the corpus but must not re-download it (~80 MB);
        # only an explicit `--force` does.
        ("chromium", chromium.generate(args.force, overwrite))
    ]:
        if kind in kinds:
            for video in generated:
                videos[kind][video.name] = video.to_json()
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
