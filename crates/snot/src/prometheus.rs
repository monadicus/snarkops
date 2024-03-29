use anyhow::bail;
use bollard::{
    container::{self, ListContainersOptions},
    image::CreateImageOptions,
    secret::HostConfig,
    Docker,
};
use futures_util::TryStreamExt;
use serde_json::json;
use tracing::info;

use crate::state::GlobalState;

const PROMETHEUS_IMAGE: &str = "prom/prometheus:latest";

const PROMETHEUS_HOST_CONFIG: &str = "prometheus.yml";
const PROMETHEUS_HOST_DATA: &str = "data";
const PROMETHEUS_CTR_CONFIG: &str = "/etc/prometheus/prometheus.yml";
const PROMETHEUS_CTR_DATA: &str = "/prometheus";

const PROMETHEUS_CTR_LABEL: &str = "snops_prometheus";
const PROMETHEUS_CTR_LABEL_VALUE: &str = "snops_prometheus=control_plane";

/// Save Prometheus config based on the current state of the control plane.
pub fn save_prometheus_config() {
    // ...
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

            // save the container ID to state
            *state.prom_ctr.lock().unwrap() = container.id.unwrap_or_default();

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
