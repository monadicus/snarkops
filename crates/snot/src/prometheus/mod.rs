mod config;

use anyhow::bail;
use bollard::{
    container::{self, ListContainersOptions},
    image::CreateImageOptions,
    secret::HostConfig,
    Docker,
};
use futures_util::TryStreamExt;
use indexmap::IndexMap;
use serde_json::json;
use tracing::info;

use self::config::{PrometheusConfig, ScrapeConfig, StaticConfig};
use crate::{env::EnvPeer, state::GlobalState};

const PROMETHEUS_IMAGE: &str = "prom/prometheus:latest";

const PROMETHEUS_HOST_CONFIG: &str = "prometheus.yml";
const PROMETHEUS_HOST_DATA: &str = "data";
const PROMETHEUS_CTR_CONFIG: &str = "/etc/prometheus/prometheus.yml";
const PROMETHEUS_CTR_DATA: &str = "/prometheus";

const PROMETHEUS_CTR_LABEL: &str = "snops_prometheus";
const PROMETHEUS_CTR_LABEL_VALUE: &str = "snops_prometheus=control_plane";

// TODO: clean this function up, possibly make config zero-copy, or use json!
// macro or something
/// Save Prometheus config based on the current state of the control plane.
pub async fn generate_prometheus_config(state: &GlobalState) -> PrometheusConfig {
    let envs = state.envs.read().await;

    let mut scrape_configs = vec![ScrapeConfig {
        job_name: "prometheus".into(),
        honor_timestamps: Some(true),
        scrape_interval: None,
        scrape_timeout: None,
        metrics_path: Some("/metrics".into()),
        scheme: Some("http".into()),
        follow_redirects: Some(true),
        static_configs: vec![StaticConfig {
            targets: vec!["localhost:9090".into()],
            labels: Default::default(),
        }],
    }];

    for env in envs.iter() {
        for (key, peer) in env.1.node_map.iter() {
            let EnvPeer::Internal(agent_id) = peer else {
                // TODO: support scraping from external peers
                continue;
            };

            let mut labels = IndexMap::new();
            labels.insert("env".into(), env.0.to_string());
            labels.insert("agent".into(), agent_id.to_string());

            // TODO: CLEANUP, possibly zero copy config
            scrape_configs.push(ScrapeConfig {
                job_name: format!("snarkos_env{}_{}", env.0, key),
                metrics_path: Some(format!("/api/v1/agents/{}/metrics", agent_id)),
                honor_timestamps: Some(true),
                scrape_interval: None,
                scrape_timeout: None,
                scheme: Some("http".into()),
                follow_redirects: Some(true),
                static_configs: vec![StaticConfig {
                    targets: vec![format!("host.docker.internal:{}", state.cli.port)],
                    labels,
                }],
            });
        }
    }

    PrometheusConfig {
        global: Default::default(),
        scrape_configs,
    }
}

/// Initialize the Prometheus container.
pub async fn init(state: &GlobalState) -> anyhow::Result<()> {
    // connect to the daemon
    let docker = Docker::connect_with_socket_defaults()?;
    let version = docker.version().await?;

    info!(
        "connected to docker daemon (version {})",
        version.version.unwrap_or_else(|| "?".into())
    );

    // check if the container is already started
    let list_options = ListContainersOptions {
        filters: [("label", vec![PROMETHEUS_CTR_LABEL_VALUE])]
            .into_iter()
            .collect(),

        ..Default::default()
    };

    let existing_containers = docker.list_containers(Some(list_options)).await?;
    match existing_containers.into_iter().next() {
        None => (),
        Some(container) => {
            // assert that the container image is prometheus
            match &container.image {
                Some(image) if image == PROMETHEUS_IMAGE => (),
                _ => bail!("found an existing matching container, but it is not prometheus"),
            }

            info!("found an existing prometheus container");

            let id = container.id.unwrap_or_default();

            // start the container if it is not already running
            match &container.state {
                Some(state) if state == "Running" => (),
                _ => {
                    docker.start_container::<&str>(&id, None).await?;
                    info!("started the matching prometheus container");
                }
            }

            // save the container ID to state
            *state.prom_ctr.lock().unwrap() = id;

            return Ok(());
        }
    }

    // pull the prometheus image
    info!("pulling prometheus image...");
    let create_image_options = CreateImageOptions {
        from_image: PROMETHEUS_IMAGE,
        ..Default::default()
    };

    docker
        .create_image(Some(create_image_options), None, None)
        .try_collect::<Vec<_>>()
        .await?;

    // create the container directories and files
    info!("setting up prometheus data...");
    let mut base = state.cli.path.canonicalize().unwrap();
    base.push("prometheus");

    let host_config = base.join(PROMETHEUS_HOST_CONFIG);
    let host_data = base.join(PROMETHEUS_HOST_DATA);

    tokio::fs::create_dir_all(&host_data).await?;
    tokio::fs::write(&host_config, "").await?;
    // TODO: create prometheus config with above function

    // create the prometheus container
    info!("creating the prometheus container...");
    let container_config = container::Config {
        image: Some(PROMETHEUS_IMAGE),

        user: Some("root"),

        // add the control plane label so we can find this container later
        labels: Some(
            [(PROMETHEUS_CTR_LABEL, "control_plane")]
                .into_iter()
                .collect(),
        ),

        host_config: Some(HostConfig {
            binds: Some(vec![
                format!(
                    "{}:{}",
                    host_config.display().to_string(),
                    PROMETHEUS_CTR_CONFIG.to_string()
                ),
                format!(
                    "{}:{}",
                    host_data.display().to_string(),
                    PROMETHEUS_CTR_DATA.to_string()
                ),
            ]),

            // TODO: expose whole host network so we can get metrics in
            port_bindings: Some(
                serde_json::from_value(
                    json!({ "9090/tcp": [{ "HostPort": state.cli.prometheus.to_string() }] }),
                )
                .unwrap(),
            ),

            ..Default::default()
        }),

        ..Default::default()
    };

    let container = docker
        .create_container::<&str, _>(None, container_config)
        .await?;

    info!("starting the prometheus container...");
    docker.start_container::<&str>(&container.id, None).await?;

    info!("started prometheus container (id {})", container.id);
    *state.prom_ctr.lock().unwrap() = container.id;

    Ok(())
}
