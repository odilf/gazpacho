"""Synthetic generated videos at a bunch of different codexs, GOPs, fps, pixel formats and so on."""

from collections.abc import Iterable, Iterator
from dataclasses import replace
from fractions import Fraction

import encode
from common import Video
from specs import Cfr, Codec, Container, PixFmt, Spec, Vfr

#: Frames per clip.
FRAMES = 60
#: `(width, height)` of the fixed matrix clips.
RESOLUTION = (160, 120)

def generate(specs: Iterable[Spec], overwrite: bool) -> Iterator[Video]:
    for spec in specs:
        yield gen_spec(overwrite, spec)

def gen_spec(overwrite: bool, spec: Spec) -> Video:
    vid = Video(
        name=spec.name(),
        category="synthetic",
        failed=False,
        meta=spec.to_json(),
    )

    if not encode.encoder_available(spec.codec.encoder()):
        vid.failed = f"encoder {spec.codec.encoder()} not available"
        return vid

    # TODO: Maybe error handle. But for now I don't know the failure cases,
    # so I'll let it loudly surface errors.
    encode.generate(spec, vid.path(), overwrite)

    return vid
    
def all_specs() -> list[Spec]:
    r30 = Fraction(30, 1)
    ntsc = Fraction(24000, 1001)
    zero = Fraction(0, 1)

    def base(codec, gop, fps, pix_fmt) -> Spec:
        return Spec(
            codec=codec,
            container=codec.default_container(),
            pix_fmt=pix_fmt,
            timing=Cfr(fps=fps),
            frames=FRAMES,
            resolution=RESOLUTION,
            gop=gop,
            bframes=0,
            start_offset=zero,
        )

    specs = []

    # Inter codecs: sweep GOP structure (all-intra / normal / longer than the
    # whole clip) against both fps (integer and NTSC rational) and both chroma
    # samplings.
    for codec in (Codec.H264, Codec.HEVC, Codec.VP9):
        for gop in (12, 1, 250):
            for fps in (r30, ntsc):
                for pix_fmt in (PixFmt.YUV420P, PixFmt.YUV444P):
                    specs.append(base(codec, gop, fps, pix_fmt))

    # FFV1 is intra-only (well, we keep it that way): lossless reference.
    for fps in (r30, ntsc):
        for pix_fmt in (PixFmt.YUV420P, PixFmt.YUV444P):
            specs.append(base(Codec.FFV1, 1, fps, pix_fmt))

    # Variable frame rate: irregular but exact millisecond durations.
    pattern = [33, 21, 100, 40, 15, 67]
    durations = [Fraction(pattern[i % len(pattern)], 1000) for i in range(FRAMES)]
    specs.append(
        replace(
            base(Codec.H264, 12, r30, PixFmt.YUV420P),
            timing=Vfr(durations=durations),
        )
    )

    # B-frames: decode order != presentation order (negative DTS / mp4 edit
    # list). The reader must hand frames back in presentation order.
    specs.append(
        replace(base(Codec.H264, 12, r30, PixFmt.YUV420P), bframes=2)
    )

    # B-frames plus a first PTS of 0.7s in mp4.
    specs.append(
        replace(
            base(Codec.H264, 12, r30, PixFmt.YUV420P),
            bframes=2,
            start_offset=Fraction(7, 10),
        )
    )

    # The classic broadcast-style case: mpegts starting at 1.4s, with
    # B-frames. `t = 0` is *before* this stream exists.
    specs.append(
        replace(
            base(Codec.H264, 12, r30, PixFmt.YUV420P),
            container=Container.MPEGTS,
            bframes=2,
            start_offset=Fraction(7, 5),
        )
    )

    return specs

