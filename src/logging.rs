use tracing;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init() {
    // Compatibility layer with `log``
    tracing_log::LogTracer::init().expect("log tracing adapter");

    // Set up the tracing subscriber to log to stdout
    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(fmt::format::FmtSpan::ACTIVE)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");
}
