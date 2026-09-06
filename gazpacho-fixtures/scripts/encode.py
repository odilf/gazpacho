"""Shared ffmpeg encoding machinery: stamping and the CFR/VFR pipelines.

A port of the encode half of the Rust `generation.rs`. Kind-specific specs and
edge files live in `synthetic.py` and `derived.py`; this module only knows how
to turn a [`Spec`](specs.Spec) into a file. Every write is temp-file-plus-
rename so a concurrently running test binary either sees the complete file or
none at all.

ffmpeg is resolved from `FFMPEG_PATH` (or `FFPROBE_PATH` for ffprobe), falling
back to `PATH`.
"""
import os
import shutil
import subprocess
from fractions import Fraction
from pathlib import Path

from common import FIXTURES_DIR
from specs import Cfr, Codec, Container, Spec

#: Stamp grid size.
GRID = 4
#: Total bits that can be stamped.
STAMP_BITS = GRID * GRID

(FIXTURES_DIR / "tmp").mkdir(parents=True, exist_ok=True)

def ffmpeg_path() -> str:
    override = os.environ.get("FFMPEG_PATH")
    if override:
        return override
    found = shutil.which("ffmpeg")
    if found:
        return found
    raise SystemExit("ffmpeg not found (install it or set FFMPEG_PATH)")


def ffprobe_path() -> str:
    override = os.environ.get("FFPROBE_PATH")
    if override:
        return override
    found = shutil.which("ffprobe")
    if found:
        return found
    raise SystemExit("ffprobe not found (install it or set FFPROBE_PATH)")


# === Stamping ===============================================================

def stamp(width: int, height: int, index: int) -> bytes:
    """A frame with `index` stamped as a `GRID`x`GRID` grid of black/white
    blocks, MSB first in raster order. The Rust `recover_index` reads this
    back: the 4x4 layout and bit order are a shared contract."""
    assert 0 <= index < 1 << STAMP_BITS, f"index {index} does not fit the stamp"
    data = bytearray(4 * width * height)
    for y in range(height):
        row = y * GRID // height
        for x in range(width):
            col = x * GRID // width
            bit = STAMP_BITS - 1 - (row * GRID + col)
            color = 255 if (index >> bit) & 1 else 0
            i = 4 * (y * width + x)
            data[i : i + 4] = bytes((color, color, color, 255))
    return bytes(data)


def write_pgm(dir_path: Path, index: int, frame_bytes: bytes, width: int, height: int) -> None:
    """Write frame `index` as an 8-bit grayscale PGM (`f{index:03}.pgm`)."""
    contents = bytearray(f"P5\n{width} {height}\n255\n".encode())
    # PGM (P5) is one byte per pixel; `stamp` returns RGBA, so take channel 0.
    contents += frame_bytes[0::4]
    (dir_path / f"f{index:03}.pgm").write_bytes(contents)


# === Encoding ===============================================================

def generate(spec: Spec, path: Path, overwrite: bool):
    """Encodes a video with stamps that indicate the frame number, in the format specified by `spec`."""
    if path.exists() and not overwrite:
        return path

    tmp = FIXTURES_DIR / "tmp" / f".{spec.name()}-{os.getpid()}.{spec.container.value}"
    try:
        if isinstance(spec.timing, Cfr):
            encode_cfr(spec, spec.timing.fps, tmp)
        else:
            encode_vfr(spec, spec.timing.durations, tmp)
    except Exception:
        tmp.unlink(missing_ok=True)
        raise
    tmp.rename(path)


def encode_cfr(spec: Spec, fps: Fraction, out: Path) -> None:
    """Constant-frame-rate encode: pipe stamped gray frames into ffmpeg's
    stdin as rawvideo."""
    cmd = [
        "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "rgba",
        "-video_size", f"{spec.resolution[0]}x{spec.resolution[1]}",
        "-framerate", f"{fps.numerator}/{fps.denominator}",
        "-i", "-",
    ]
    cmd += codec_args(spec)
    cmd += output_args(spec)
    cmd.append(str(out))
    run_feeding_frames(cmd, spec)


def encode_vfr(spec: Spec, durations: list[Fraction], out: Path) -> None:
    """Variable-frame-rate encode with *exact* per-frame timestamps.

    Two passes: pass 1 appends a throwaway sentinel frame at the stream's end
    (giving the last real frame a successor, from which its display duration
    is inferred), rewriting each frame's PTS exactly with a `setpts` lookup;
    pass 2 (`-c copy`) drops the sentinel by keeping exactly `frames` frames.
    """
    out = Path(out)
    assert len(durations) == spec.frames, "need one duration per frame"
    staging = out.with_suffix(".frames")
    intermediate = out.with_suffix(".inter.mp4")
    staging.mkdir(parents=True, exist_ok=True)

    try:
        # Prefix sums in whole milliseconds: `prefix[k]` is frame `k`'s
        # presentation time, and `prefix[frames]` is the stream's end (where
        # the sentinel goes).
        prefix = [0]
        for duration in durations:
            prefix.append(prefix[-1] + duration_millis(duration))

        # Real frames `f000..`, then one sentinel at `f{frames}`. The
        # sentinel's pixels never survive pass 2, so reuse frame 0's stamp.
        width, height = spec.resolution
        for i in range(spec.frames):
            write_pgm(staging, i, stamp(width, height, i), width, height)
        write_pgm(staging, spec.frames, stamp(width, height, 0), width, height)

        # `setpts` in a 1 ms timebase: `N -> prefix[N]`. Commas inside the
        # expression are escaped so libavfilter doesn't read them as filter
        # separators; a filter-script file also sidesteps shell quoting.
        filter_expr = "settb=1/1000,setpts=" + "+".join(
            f"eq(N\\,{n})*{ms}" for n, ms in enumerate(prefix)
        )
        script = staging / "setpts.txt"
        script.write_text(filter_expr)

        # Pass 1: encode every frame (real + sentinel), rewriting timestamps.
        run_ffmpeg(
            [
                "-hide_banner", "-loglevel", "error", "-y",
                "-framerate", "1000",
                "-i", str(staging / "f%03d.pgm"),
                "-filter_script:v", str(script),
            ]
            + codec_args(spec)
            + ["-fps_mode", "passthrough", "-video_track_timescale", "1000", str(intermediate)]
        )

        # Pass 2: drop the sentinel by copying exactly `frames` frames. Stream
        # copy keeps each sample's stored duration, so the last real frame
        # retains the duration the sentinel gave it. Container/offset options
        # apply to this real output.
        run_ffmpeg(
            [
                "-hide_banner", "-loglevel", "error", "-y",
                "-i", str(intermediate),
                "-c", "copy",
                "-frames:v", str(spec.frames),
                "-video_track_timescale", "1000",
            ]
            + output_args(spec)
            + [str(out)]
        )
    finally:
        shutil.rmtree(staging, ignore_errors=True)
        intermediate.unlink(missing_ok=True)


def duration_millis(duration: Fraction) -> int:
    ms = duration * Fraction(1000, 1)
    if ms.denominator != 1:
        raise ValueError(f"duration {duration}s is not a whole millisecond")
    return ms.numerator


def codec_args(spec: Spec) -> list[str]:
    gop = str(spec.gop)
    if spec.codec == Codec.H264:
        args = [
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "18",
            "-g", gop, "-keyint_min", gop,
            "-bf", str(spec.bframes),
            # Exact GOP placement: no scene-cut keyframes.
            "-x264-params", "scenecut=0",
        ]
    elif spec.codec == Codec.HEVC:
        # Ultrafast messes up, for instance, `rand_000000006a5a9ac0_04.ts`.
        args = [
            "-c:v", "libx265", "-preset", "fast", "-crf", "12",
            "-x265-params",
            f"keyint={gop}:min-keyint={gop}:scenecut=0:bframes={spec.bframes}:log-level=error",
        ]
    elif spec.codec == Codec.VP9:
        args = [
            "-c:v", "libvpx-vp9",
            "-deadline", "realtime", "-cpu-used", "8",
            "-crf", "32", "-b:v", "0",
            "-g", gop,
        ]
    else:  # FFV1
        args = ["-c:v", "ffv1", "-level", "3", "-g", "1"]
    return args + ["-pix_fmt", spec.pix_fmt.value]


def output_args(spec: Spec) -> list[str]:
    """Container/timestamp options: start offsets and mpegts determinism."""
    args = []
    if spec.container == Container.MPEGTS:
        # Kill the mpegts muxer's default ~1.4s preload delay so the start
        # offset is *exactly* the first PTS.
        args += ["-muxdelay", "0", "-muxpreload", "0"]
    if spec.container == Container.MP4 and isinstance(spec.timing, Cfr):
        # A timescale in which both the frame duration (1/fps) and any
        # whole-millisecond start offset are exact. The VFR pipeline already
        # forces a 1 kHz timescale.
        num = spec.timing.fps.numerator
        args += ["-video_track_timescale", str(1000 // gcd(1000, num) * num)]
    if spec.start_offset != Fraction(0, 1):
        args += ["-output_ts_offset", format_seconds_signed(spec.start_offset)]
    return args


def gcd(a: int, b: int) -> int:
    while b:
        a, b = b, a % b
    return a


def format_seconds_signed(t: Fraction) -> str:
    """Format an exact rational second count as a decimal string ffmpeg parses
    back exactly (ffmpeg time parsing is decimal microseconds, not float)."""
    micros = t * Fraction(1_000_000, 1)
    if micros.denominator != 1:
        raise ValueError(f"time {t} is not representable in whole microseconds")
    micros = micros.numerator
    sign = "-" if micros < 0 else ""
    micros = abs(micros)
    return f"{sign}{micros // 1_000_000}.{micros % 1_000_000:06d}"


def run_feeding_frames(args: list[str], spec: Spec) -> None:
    frames = b"".join(stamp(*spec.resolution, i) for i in range(spec.frames))
    run_ffmpeg(args, input_bytes=frames)


def run_ffmpeg(args: list[str], input_bytes: bytes | None = None) -> None:
    print(f"encoding '{args[-1]}'")
    proc = subprocess.run(
        [ffmpeg_path(), *args],
        input=input_bytes,
        stdin=subprocess.DEVNULL if input_bytes is None else None,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"ffmpeg failed ({proc.returncode}): "
            f"{proc.stderr.decode(errors='replace').strip()}"
        )


def get_encoders():
    proc = subprocess.run(
        [ffmpeg_path(), "-hide_banner", "-encoders"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    # Lines look like ` V....D libx264  H.264 / ...`.
    return {
        line.split()[1]
        for line in proc.stdout.decode(errors="replace").splitlines()
        if len(line.split()) >= 2
    }
    
ENCODERS = get_encoders()

def encoder_available(name: str) -> bool:
    return name in ENCODERS
