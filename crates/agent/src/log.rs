use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, reload, util::SubscriberInitExt};

pub type ReloadHandler = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

pub fn make_env_filter(level: LevelFilter) -> EnvFilter {
    EnvFilter::builder()
        .with_env_var("SNOPS_AGENT_LOG")
        .with_default_directive(level.into())
        .from_env_lossy()
        .add_directive(level.into())
        .add_directive("neli=off".parse().unwrap())
        .add_directive("hyper_util=off".parse().unwrap())
        .add_directive("reqwest=off".parse().unwrap())
        .add_directive("tungstenite=off".parse().unwrap())
        .add_directive("tokio_tungstenite=off".parse().unwrap())
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap())
}

pub fn init_logging() -> (WorkerGuard, ReloadHandler) {
    let (stdout, guard) = tracing_appender::non_blocking(std::io::stdout());

    let output: tracing_subscriber::fmt::Layer<
        _,
        tracing_subscriber::fmt::format::DefaultFields,
        tracing_subscriber::fmt::format::Format,
        tracing_appender::non_blocking::NonBlocking,
    > = tracing_subscriber::fmt::layer().with_writer(stdout);

    let output = if cfg!(debug_assertions) {
        output.with_file(true).with_line_number(true)
    } else {
        output
    };

    let filter_level = if cfg!(debug_assertions) {
        LevelFilter::TRACE
    } else {
        LevelFilter::INFO
    };

    let (env_filter, reload_handler) = reload::Layer::new(make_env_filter(filter_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(output)
        .try_init()
        .unwrap();

    (guard, reload_handler)
}
