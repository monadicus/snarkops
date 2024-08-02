use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::bail;
use chrono::Utc;
use futures::StreamExt;
use http::StatusCode;
use reqwest::IntoUrl;
use sha2::{Digest, Sha256};
use snops_common::{
    binaries::{BinaryEntry, BinarySource},
    state::TransferStatusUpdate,
};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::info;

use crate::transfers::{self, TransferTx};

const TRANSFER_UPDATE_RATE: Duration = Duration::from_secs(2);

/// Download a file. Returns a None if 404.
pub async fn download_file(
    client: &reqwest::Client,
    url: impl IntoUrl,
    to: impl AsRef<Path>,
    transfer_tx: TransferTx,
) -> anyhow::Result<Option<(File, String, u64)>> {
    let desc = url.as_str().to_owned();
    let req = client.get(url).send().await?;
    if req.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    // create a new transfer
    let tx_id = transfers::next_id();
    transfer_tx.send((
        tx_id,
        TransferStatusUpdate::Start {
            desc,
            time: Utc::now(),
            total: req.content_length().unwrap_or_default(),
        },
    ))?;

    let mut stream = req.bytes_stream();
    let mut file = File::create(to).await.inspect_err(|_| {
        let _ = transfer_tx.send((
            tx_id,
            TransferStatusUpdate::End {
                interruption: Some("failed to create file".to_string()),
            },
        ));
    })?;

    let mut downloaded = 0;
    let mut digest = Sha256::new();
    let mut update_next = Instant::now() + TRANSFER_UPDATE_RATE;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.inspect_err(|e| {
            let _ = transfer_tx.send((
                tx_id,
                TransferStatusUpdate::End {
                    interruption: Some(format!("stream error: {e:?}")),
                },
            ));
        })?;

        downloaded += chunk.len() as u64;
        digest.update(&chunk);

        // update the transfer if the update interval has elapsed
        let now = Instant::now();
        if now > update_next {
            update_next = now + TRANSFER_UPDATE_RATE;
            let _ = transfer_tx.send((tx_id, TransferStatusUpdate::Progress { downloaded }));
        }

        file.write_all(&chunk).await.inspect_err(|e| {
            let _ = transfer_tx.send((
                tx_id,
                TransferStatusUpdate::End {
                    interruption: Some(format!("write error: {e:?}")),
                },
            ));
        })?;
    }

    let sha256 = format!("{:x}", digest.finalize());

    // mark the transfer as ended
    transfer_tx.send((tx_id, TransferStatusUpdate::End { interruption: None }))?;

    Ok(Some((file, sha256, downloaded)))
}

pub async fn check_file(
    url: impl IntoUrl,
    to: &Path,
    transfer_tx: TransferTx,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    if !should_download_file(&client, url.as_str(), to)
        .await
        .unwrap_or(true)
    {
        return Ok(());
    }

    info!("downloading {to:?}");
    download_file(&client, url, to, transfer_tx).await?;

    Ok(())
}

pub async fn check_binary(
    binary: &BinaryEntry,
    base_url: &str,
    path: &Path,
    transfer_tx: TransferTx,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    // check if we already have an up-to-date binary
    let source_url = match &binary.source {
        BinarySource::Url(url) => url.to_string(),
        BinarySource::Path(path) => {
            format!("{base_url}{}", path.display())
        }
    };

    // TODO: check binary size and shasum if provided

    if !should_download_file(&client, &source_url, path)
        .await
        .unwrap_or(true)
    {
        // check permissions and ensure 0o755
        let perms = path.metadata()?.permissions();
        if perms.mode() != 0o755 {
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).await?;
        }

        // TODO: check sha256 and size

        return Ok(());
    }
    info!("binary update is available, downloading...");

    let Some((file, _sha256, _size)) =
        download_file(&client, &source_url, path, transfer_tx).await?
    else {
        bail!("downloading binary returned 404");
    };

    // TODO: check sha256 and size

    // ensure the permissions are set for execution
    file.set_permissions(std::fs::Permissions::from_mode(0o755))
        .await?;

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
