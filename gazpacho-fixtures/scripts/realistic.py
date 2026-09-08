"""
- Blender open-movie clips (served at small sizes by `test-videos.co.uk`)
- public-domain live-action clips from Wikimedia Commons.
"""

import os
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, NamedTuple

from chromium import decodes_cleanly, ext_of, has_video_packets, sha256_of
from common import FIXTURES_DIR, Category, Json, Video
from encode import probe_cost

#: Some sources (test-videos.co.uk) 403 the default `Python-urllib` user-agent,
#: so downloads identify as a stock browser.
USER_AGENT = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/120.0 Safari/537.36"
)


@dataclass(frozen=True)
class Clip:
    """One downloadable clip: its fixture name, a human-readable source for
    attribution, and the URL to fetch it from."""

    name: str
    source: str
    url: str


#: The curated catalog. Names are the filenames stored under `realistic/`;
#: they must stay unique across the whole manifest.
CATALOG = [
    # Blender open movies, re-encoded to small 10s clips by test-videos.co.uk.
    Clip(
        name="bigbuckbunny_720_10s_10MB.mp4",
        source="test-videos.co.uk (Big Buck Bunny, Blender Foundation, CC-BY)",
        url=(
            "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/720/"
            "Big_Buck_Bunny_720_10s_10MB.mp4"
        ),
    ),
    Clip(
        name="sintel_720_10s_5MB.mp4",
        source="test-videos.co.uk (Sintel, Blender Foundation, CC-BY)",
        url=(
            "https://test-videos.co.uk/vids/sintel/mp4/h264/720/"
            "Sintel_720_10s_5MB.mp4"
        ),
    ),
    Clip(
        name="jellyfish_1080_10s_10MB.mp4",
        source="test-videos.co.uk (Jellyfish, Blender Foundation)",
        url=(
            "https://test-videos.co.uk/vids/jellyfish/mp4/h264/1080/"
            "Jellyfish_1080_10s_10MB.mp4"
        ),
    ),
    # Public-domain live-action footage from Wikimedia Commons. `Special:FilePath`
    # redirects to the current canonical upload, so the download URL is stable.
    Clip(
        name="the_cook_1918.webm",
        source="Wikimedia Commons (The Cook, 1918 — public domain)",
        url="https://commons.wikimedia.org/wiki/Special:FilePath/The_Cook_(clip,_1918).webm",
    ),
    Clip(
        name="bombing_of_hamburg.ogv",
        source="Wikimedia Commons (Bombing of Hamburg, US government, public domain)",
        url="https://commons.wikimedia.org/wiki/Special:FilePath/Bombing_of_Hamburg.ogv",
    ),
]


class Annotation(NamedTuple):
    sha256: str
    size: int
    decodes_cleanly: bool
    has_video_packets: bool
    extension: str
    cost: int

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


def generate(force_download: bool=False) -> list[Video]:
    root = FIXTURES_DIR / "realistic"
    root.mkdir(parents=True, exist_ok=True)

    videos = []
    for clip in CATALOG:
        path = root / clip.name
        if not path.exists() or force_download:
            download(clip, path)
        annotation = annotate(path)
        videos.append(
            Video(
                name=clip.name,
                category=Category.REALISTIC,
                failed=annotation.fails(),
                # Failed videos cost nothing to run: they surface as ignored.
                cost=annotation.cost,
                meta={
                    "source": clip.source,
                    **annotation.to_json(),
                },
            )
        )
    return videos


def download(clip: Clip, path: Path) -> None:
    print(f"downloading {clip.name} from {clip.source}")
    tmp = FIXTURES_DIR / f".realistic-{os.getpid()}-{clip.name}"
    request = urllib.request.Request(clip.url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request) as response:
            tmp.write_bytes(response.read())
        tmp.rename(path)
    finally:
        tmp.unlink(missing_ok=True)


def annotate(path: Path) -> Annotation:
    return Annotation(
        sha256=sha256_of(path),
        size=path.stat().st_size,
        decodes_cleanly=decodes_cleanly(path),
        has_video_packets=has_video_packets(path),
        extension=ext_of(path.name),
        cost=probe_cost(path),
    )
