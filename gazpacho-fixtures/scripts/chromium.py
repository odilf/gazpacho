"""The Chromium media test corpus (`media/test/data` in the Chromium tree),
downloaded once and cached under `target/gazpacho-fixtures/chromium/`.

Sources come from `chromium.googlesource.com`, which serves a tar.gz of just
that directory, pinned on a commit. The archive is extracted into the cache
directory as-is, then every file is annotated with its sha256, size, and whether
ffmpeg can decode it as a video (the corpus contains encrypted, corrupted,
truncated, and audio-only files, so validation is one-time and cached).
"""

import concurrent.futures
import csv
import hashlib
import io
import os
import shutil
import subprocess
import tarfile
import urllib.request
from pathlib import Path
from typing import Literal, NamedTuple

from common import FIXTURES_DIR, Json, Video
from encode import ffmpeg_path, ffprobe_path

#: The pinned Chromium commit the corpus is downloaded at.
#:
#: To find the latest commit:
#: `curl -s "https://api.github.com/repos/chromium/chromium/commits?path=media/test/data&per_page=1"`
COMMIT = "acb10adca5300302643fa4014825eae9ceaf7adc"

#: Per-file annotation cache. Bump the version to re-run hashing/decode
#: validation without re-downloading the data.
MANIFEST = "manifest-2026-09-06.txt"

#: Worker threads for the one-time decode validation pass.
VALIDATE_THREADS = 8

VIDEO_EXTENSIONS = {"mp4", "mkv", "webm", "mov", "ts", "m4v", "avi", "ogv"}


class Annotation(NamedTuple):
    """One corpus file's annotation: its path relative to the corpus root,
    plus the validation metadata (hash, size, decodability, extension)."""

    rel: str
    """Path of the file relative to the corpus root, with POSIX separators;
    doubles as the generated `Video`'s name."""

    sha256: str
    size: int
    decodes_cleanly: bool
    has_video_packets: bool
    extension: str

    def to_json(self) -> dict[str, Json]:
        return {
            "sha256": self.sha256,
            "size": self.size,
            "decodes_cleanly": self.decodes_cleanly,
            "has_video_packets": self.has_video_packets,
            "extension": self.extension,
        }

    def fails(self) -> Literal[False] | str:
        if not self.decodes_cleanly:
            return "ffmpeg can't decode file cleanly"
        elif not self.has_video_packets:
            return "no video packets in video"
        else:
            return False


def generate(force_download: bool, force_retag: bool) -> list[Video]:
    """One `Video` per corpus file.

    `force_download` re-downloads and re-extracts the archive even when the
    recorded commit matches the pin; `force_retag` re-runs the one-time
    hashing/decode-annotation pass even when a cached manifest exists. A
    missing or mismatched `.commit` always re-downloads; a missing or
    unparseable manifest always re-annotates.
    """
    root = FIXTURES_DIR / "chromium"
    root.mkdir(parents=True, exist_ok=True)

    # Re-download only on explicit request or when the recorded commit differs
    # from the pin (missing `.commit` counts as different). A change in the
    # generation code alone must never trigger an ~80 MB re-download.
    commit = root / ".commit"
    recorded = commit.read_text().strip() if commit.exists() else None
    if force_download or recorded != COMMIT:
        if recorded != COMMIT:
            print("Chromium commit pin changed; re-downloading corpus")
        download_and_extract(root)
        commit.write_text(COMMIT)

    annotations = load_or_build_annotations(root, force_retag)

    return [
        Video(
            name=annotation.rel,
            category="chromium",
            failed=annotation.fails(),
            meta=annotation.to_json(),
        )
        for annotation in annotations
        if annotation.extension in VIDEO_EXTENSIONS
    ]


def download_and_extract(root: Path) -> None:
    url = (
        "https://chromium.googlesource.com/chromium/src/+archive/"
        f"{COMMIT}/media/test/data.tar.gz"
    )
    print(f"downloading Chromium media test corpus (~80 MB): {url}")

    pid = os.getpid()
    tarball = FIXTURES_DIR / f".chromium-{pid}.tar.gz"
    staging = FIXTURES_DIR / f".chromium-{pid}"
    try:
        urllib.request.urlretrieve(url, tarball)
        staging.mkdir(parents=True, exist_ok=True)
        with tarfile.open(tarball, "r:gz") as tf:
            tf.extractall(staging)
        if root.exists():
            shutil.rmtree(root, ignore_errors=True)
        try:
            staging.rename(root)
        except FileExistsError:
            # Lost the race with a concurrent test binary; its copy is
            # complete because dir renames are atomic.
            shutil.rmtree(staging, ignore_errors=True)
    finally:
        tarball.unlink(missing_ok=True)
        if not root.exists():
            shutil.rmtree(staging, ignore_errors=True)


def load_or_build_annotations(
    root: Path, force_retag: bool = False
) -> list[Annotation]:
    cache = root / MANIFEST
    if not force_retag and cache.exists() and (rows := read_cache(cache)) is not None:
        return rows

    rows = annotate_all(root)
    # Temp-plus-rename, like the generated fixtures.
    tmp = root / f".manifest-{os.getpid()}"
    tmp.write_text(serialize_cache(rows))
    tmp.rename(cache)
    return rows


def annotate_all(root: Path) -> list[Annotation]:
    """Annotate every file under the corpus root, sorted by name (registry
    order must be deterministic for reproducible samples). Our own bookkeeping
    (`.commit`, the manifest cache, temp files) is excluded."""
    files = sorted(
        path
        for path in root.rglob("*")
        if path.is_file() and not path.name.startswith(".") and path.name != MANIFEST
    )
    print(f"annotating {len(files)} Chromium corpus files ")

    rows: list[Annotation] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=VALIDATE_THREADS) as pool:
        futures = {pool.submit(annotate, root, path): path for path in files}
        for future in concurrent.futures.as_completed(futures):
            rows.append(future.result())
    rows.sort(key=lambda row: row.rel)
    return rows


def annotate(root: Path, path: Path) -> Annotation:
    rel = path.relative_to(root).as_posix()
    return Annotation(
        rel=rel,
        sha256=sha256_of(path),
        size=path.stat().st_size,
        decodes_cleanly=decodes_cleanly(path),
        has_video_packets=has_video_packets(path),
        extension=ext_of(rel),
    )


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ext_of(rel: str) -> str:
    suffix = Path(rel).suffix
    return suffix[1:].lower() or "bin"


def serialize_cache(rows: list[Annotation]) -> str:
    buf = io.StringIO()
    writer = csv.writer(buf, delimiter="\t", lineterminator="\n")
    writer.writerows(
        [row.rel, row.sha256, row.size, 1 if row.decodes_cleanly else 0, 1 if row.has_video_packets else 0] for row in rows
    )
    return buf.getvalue()


def read_cache(path: Path) -> list[Annotation] | None:
    rows: list[Annotation] = []
    with path.open(newline="") as f:
        for parts in csv.reader(f, delimiter="\t"):
            if not parts:
                continue
            if len(parts) != len(Annotation._fields) - 1:
                return None
            rel, sha, size, decodes_cleanly, has_video_packets = parts
            rows.append(
                Annotation(
                    rel=rel,
                    sha256=sha,
                    size=int(size),
                    decodes_cleanly=decodes_cleanly == "1",
                    has_video_packets=has_video_packets == "1",
                    extension=ext_of(rel),
                )
            )
    return rows



def decodes_cleanly(path: Path) -> bool:
    """Whether ffmpeg decodes the file's first video stream start to finish
    without a single error."""
    proc = subprocess.run(
        [
            ffmpeg_path(),
            "-hide_banner",
            "-loglevel",
            "error",
            "-xerror",
            "-i",
            str(path),
            "-map",
            "0:v:0",
            "-f",
            "null",
            "-",
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return proc.returncode == 0


def has_video_packets(path: Path) -> bool:
    """Whether the file's first video stream carries any packets at all —
    rejects metadata-track / init-segment-style files."""
    proc = subprocess.run(
        [
            ffprobe_path(),
            "-loglevel",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts",
            "-of",
            "csv=p=0",
            # Stop after the first packet; existence is all that matters.
            "-read_intervals",
            "%+#1",
            str(path),
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return proc.returncode == 0 and bool(proc.stdout.strip())
