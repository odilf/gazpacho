use tracing_subscriber::{EnvFilter, fmt::TestWriter};

pub mod video;

pub use video::videos;

// TODO: I feel I should delete this.
/// Tracing for tests compatible with libtest-mimic harnesses.
#[track_caller]
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(TestWriter::with_stderr())
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

/// Like [`init_tracing`], but writing to stderr. Needed for custom test
/// harnesses.
#[track_caller]
pub fn init_tracing_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(std::io::stderr)
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("gazpacho_fixtures=warn,gazpacho_media=debug"))
}
