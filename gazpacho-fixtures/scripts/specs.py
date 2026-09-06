"""Shared ground-truth types for the fixture corpus.

`Spec` and friends are the single source of truth: each kind module
(`synthetic.py`) builds specs from these types, and `generate.py` encodes and
serializes them into `manifest.json`, which the Rust test crate deserializes
back into its own spec types. Rationals are `fractions.Fraction` here and
`[num, denom]` pairs in JSON.
"""

from dataclasses import dataclass
from enum import Enum
from fractions import Fraction

from common import Json


class Codec(Enum):
    H264 = "h264"
    HEVC = "hevc"
    VP9 = "vp9"
    FFV1 = "ffv1"
    AV1 = "av1"

    def encoder(self) -> str:
        return {
            Codec.H264: "libx264",
            Codec.HEVC: "libx265",
            Codec.VP9: "libvpx-vp9",
            Codec.FFV1: "ffv1",
            Codec.AV1: "libaom-av1",
        }[self]

    def default_container(self) -> "Container":
        return {
            Codec.H264: Container.MP4,
            Codec.HEVC: Container.MP4,
            Codec.VP9: Container.WEBM,
            Codec.FFV1: Container.MKV,
            Codec.AV1: Container.MKV,
        }[self]


class PixFmt(Enum):
    YUV420P = "yuv420p"
    YUV444P = "yuv444p"

    def tag(self) -> str:
        return {PixFmt.YUV420P: "420p", PixFmt.YUV444P: "444p"}[self]


class Container(Enum):
    MP4 = "mp4"
    MKV = "mkv"
    WEBM = "webm"
    MPEGTS = "ts"


@dataclass(frozen=True)
class Cfr:
    fps: Fraction


@dataclass(frozen=True)
class Vfr:
    durations: list[Fraction]


@dataclass(frozen=True)
class Spec:
    # name: str
    # kind: str
    codec: Codec
    container: Container
    pix_fmt: PixFmt
    timing: Cfr | Vfr
    frames: int
    resolution: tuple[int, int]
    #: Forced keyframe interval; 1 = all-intra.
    gop: int
    #: Max consecutive B-frames (H.264/HEVC only).
    bframes: int
    #: Timestamp of the first frame, in seconds. Nonzero for the mpegts-style
    #: fixtures where the stream does not start at t = 0.
    start_offset: Fraction

    def name(self) -> str:
        parts = [self.codec.value, self.pix_fmt.tag(), f"g{self.gop}", fps_tag(self.timing)]
        if self.bframes != 0:
            parts.append(f"b{self.bframes}")
        if self.start_offset != 0:
            parts.append("offset")

        return f"{'_'.join(parts)}.{self.container.value}"

    def to_json(self) -> dict[str, Json]:
        return {
            "codec": self.codec.value,
            "container": self.container.value,
            "pix_fmt": self.pix_fmt.value,
            "timing": timing_to_json(self.timing),
            "frames": self.frames,
            "resolution": [self.resolution[0], self.resolution[1]],
            "gop": self.gop,
            "bframes": self.bframes,
            "start_offset": ratio_to_json(self.start_offset),
        }


def ratio_to_json(value: Fraction) -> list[int]:
    return [value.numerator, value.denominator]


def timing_to_json(timing) -> dict:
    if isinstance(timing, Cfr):
        return {"cfr": {"fps": ratio_to_json(timing.fps)}}
    return {"vfr": {"durations": [ratio_to_json(d) for d in timing.durations]}}


def fps_tag(timing: Cfr | Vfr) -> str:
    if isinstance(timing, Vfr):
        return "vfr"

    fps = timing.fps
    if fps.denominator == 1:
        return str(fps.numerator)
    if fps == Fraction(24000, 1001):
        return "ntsc"
    return f"{fps.numerator}-{fps.denominator}"
