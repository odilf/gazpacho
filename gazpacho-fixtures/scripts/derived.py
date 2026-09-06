"""Edge-case fixtures derived from the baseline clip.

A port of the `generation.rs` derived section. These one-off files don't fit
the `Spec` matrix but exercise real-world metadata quirks: a trimming edit
list, an audio track, and cover art. Each is built from the baseline clip and
cached on disk like the generated specs.
"""

import os
from collections.abc import Iterator
from pathlib import Path

import synthetic
from common import Video
from encode import run_ffmpeg

BASELINE = synthetic.all_specs()[0]


def generate(overwrite: bool) -> Iterator[Video]:
    baseline = synthetic.gen_spec(spec=BASELINE, overwrite=False)
    variants = [
        ("trimmed", trimmed),
        ("with_audio", with_audio),
        ("with_cover", with_cover_art),
    ]
    for name, build in variants:
        vid = Video(
            name=f"{name}__{baseline.name}",
            category="derived",
            failed=False,
            meta={
                "baseline": baseline.to_json(),
                "baseline_path": str(baseline.path()),
                "edit": name,
            },
        )

        if vid.path().exists() and not overwrite:
            yield vid
            continue

        tmp = vid.path().parent / f".encoding-derived-{os.getpid()}-{vid.name}"
        try:
            build(baseline.path(), tmp)
        except Exception:
            tmp.unlink(missing_ok=True)
            raise
        tmp.rename(vid.path())
        yield vid


def trimmed(base: Path, out: Path) -> None:
    """Trims a video to a non(-necessarily)-keyframe start, producing an mp4
    with a *trimming edit list*."""
    run_ffmpeg(
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            "0.2",
            "-i",
            str(base),
            "-map",
            "0:v:0",
            "-c",
            "copy",
            str(out),
        ]
    )


def with_audio(base: Path, out: Path) -> None:
    """Adds a stereo 44.1 kHz silent AAC audio track."""
    run_ffmpeg(
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(base),
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-shortest",
            str(out),
        ]
    )


def with_cover_art(base: Path, out: Path) -> None:
    """Adds cover art as an `attached_pic` disposition video stream."""
    out = Path(out)
    cover = out.with_suffix(".cover.png")
    run_ffmpeg(
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:d=1",
            "-frames:v",
            "1",
            str(cover),
        ]
    )
    try:
        run_ffmpeg(
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(base),
                "-i",
                str(cover),
                "-map",
                "0:v",
                "-map",
                "1:v",
                "-c",
                "copy",
                "-disposition:v:1",
                "attached_pic",
                str(out),
            ]
        )
    finally:
        cover.unlink(missing_ok=True)
