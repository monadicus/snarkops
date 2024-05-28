use std::{os::unix::fs::PermissionsExt, path::Path};

use futures::StreamExt;
use http::StatusCode;
use reqwest::IntoUrl;
use snops_common::{api::EnvInfo, state::EnvId};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::info;

/// Download a file. Returns a None if 404.
pub async fn download_file(
    client: &reqwest::Client,
    url: impl IntoUrl,
    to: impl AsRef<Path>,
) -> anyhow::Result<Option<()>> {
    let req = client.get(url).send().await?;
    if req.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    let mut stream = req.bytes_stream();
    let mut file = File::create(to).await?;

    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }

    Ok(Some(()))
}

pub async fn check_file(url: impl IntoUrl, to: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    if !should_download_file(&client, url.as_str(), to)
        .await
        .unwrap_or(true)
    {
        return Ok(());
    }

    info!("downloading {to:?}");
    download_file(&client, url, to).await?;

    Ok(())
}

pub async fn get_env_info(url: impl IntoUrl) -> anyhow::Result<EnvInfo> {
    let req = reqwest::get(url).await?;
    if !req.status().is_success() {
        return Err(anyhow::anyhow!(
            "error getting storage info: {}",
            req.status()
        ));
    }
    let body = req.json().await?;
    Ok(body)
}

pub async fn check_binary(env_id: EnvId, base_url: &str, path: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    // check if we already have an up-to-date binary
    let loc = format!("{base_url}/api/v1/env/{env_id}/storage/binary");
    if !should_download_file(&client, &loc, path)
        .await
        .unwrap_or(true)
    {
        return Ok(());
    }
    info!("binary update is available, downloading...");

    // download the binary
    let mut file = tokio::fs::File::create(path).await?;
    let mut stream = client.get(&loc).send().await?.bytes_stream();

    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }

    // ensure the permissions are set
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).await?;

    Ok(())
}

pub async fn should_download_file(
    client: &reqwest::Client,
    loc: &str,
    path: &Path,
) -> anyhow::Result<bool> {
    Ok(match tokio::fs::metadata(&path).await {
        Ok(meta) => {
            // check last modified
            let res = client.head(loc).send().await?;

            let Some(last_modified_header) = res.headers().get(http::header::LAST_MODIFIED) else {
                return Ok(true);
            };

            let Some(content_length_header) = res.headers().get(http::header::CONTENT_LENGTH)
            else {
                return Ok(true);
            };

            let remote_last_modified = httpdate::parse_http_date(last_modified_header.to_str()?)?;
            let local_last_modified = meta.modified()?;

            let remote_content_length = content_length_header.to_str()?.parse::<u64>()?;
            let local_content_length = meta.len();

            remote_last_modified > local_last_modified
                || remote_content_length != local_content_length
        }

        // no existing file, unconditionally download binary
        Err(_) => true,
    })
}
