use eyre::{WrapErr as _, ensure};
use gazpacho_fixtures::{
    props, test_video_properties,
    video::{SyntheticVideo, decode_all_rgba},
    videos,
};

test_video_properties! {
    props!([stamp_survives_encode_decode, 2], videos().synthetic.iter().collect())
}

/// Whether stamp survives after a full encode/decode round trip through
/// an *independent* ffmpeg pipe; i.e., every frame still announces its index, in
/// presentation order. Covers the lossiest codec, VFR, B-frame reordering, and
/// the seeded random specs.
fn stamp_survives_encode_decode(video: &SyntheticVideo) -> eyre::Result<()> {
    let spec = &video.meta;
    let frames = decode_all_rgba(&video.path, spec.resolution)?;
    ensure!(
        frames.len() == spec.frames as usize,
        "expected {} frames, got {}",
        spec.frames,
        frames.len()
    );
    for (i, frame) in frames.iter().enumerate() {
        let recovered = frame
            .recover_index()
            .wrap_err_with(|| format!("frame {i}"))?;
        ensure!(
            recovered == i as u32,
            "frame {i}: expected index {i}, got {recovered}"
        );
    }

    Ok(())
}
