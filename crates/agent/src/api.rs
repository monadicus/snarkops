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
    rpc::error::ReconcileError2,
    state::{TransferId, TransferStatusUpdate},
    util::sha256_file,
};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::info;

use crate::transfers::{self, TransferTx};

const TRANSFER_UPDATE_RATE: Duration = Duration::from_secs(2);

/// Download a file. Returns a None if 404.
pub async fn download_file(
    tx_id: TransferId,
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

    // start a new transfer
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

    // this also checks for sha256 differences, along with last modified time
    // against the target
    if !get_file_issues(
        &client,
        &source_url,
        path,
        binary.size,
        binary.sha256.as_deref(),
        false,
    )
    .await
    .map(|e| e.is_none())
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
    info!("downloading binary update to {}: {binary}", path.display());

    let tx_id = transfers::next_id();
    let Some((file, sha256, size)) =
        download_file(tx_id, &client, &source_url, path, transfer_tx).await?
    else {
        bail!("downloading binary returned 404");
    };

    if let Some(bin_sha256) = &binary.sha256 {
        if sha256 != bin_sha256.to_ascii_lowercase() {
            bail!(
                "binary sha256 mismatch for {}: expected {}, found {}",
                path.display(),
                bin_sha256,
                sha256
            );
        }
    }

    if let Some(bin_size) = binary.size {
        if size != bin_size {
            bail!(
                "binary size mismatch for {}: expected {}, found {}",
                path.display(),
                bin_size,
                size
            );
        }
    }

    // ensure the permissions are set for execution
    file.set_permissions(std::fs::Permissions::from_mode(0o755))
        .await?;

    Ok(())
}

#[derive(Debug)]
pub enum BadFileReason {
    /// File is missing
    NotFound,
    /// File size mismatch
    Size,
    /// SHA256 mismatch
    Sha256,
    /// A new version is available based on modified header
    Stale,
}

pub async fn get_file_issues(
    client: &reqwest::Client,
    src: &str,
    dst: &Path,
    size: Option<u64>,
    sha256: Option<&str>,
    offline: bool,
) -> Result<Option<BadFileReason>, ReconcileError2> {
    if !dst.try_exists().unwrap_or(false) {
        return Ok(Some(BadFileReason::NotFound));
    }

    let meta = tokio::fs::metadata(&dst)
        .await
        .map_err(|e| ReconcileError2::FileStatError(dst.to_path_buf(), e.to_string()))?;
    let local_content_length = meta.len();

    // if the binary entry is provided, check if the file size and sha256 match
    // file size is incorrect
    if size.is_some_and(|s| s != local_content_length) {
        return Ok(Some(BadFileReason::Size));
    }

    // if sha256 is present, only download if the sha256 is different
    if let Some(sha256) = sha256 {
        let bad_sha256 = sha256_file(&dst.to_path_buf())
            .map_err(|e| ReconcileError2::FileReadError(dst.to_path_buf(), e.to_string()))?
            != sha256.to_ascii_lowercase();
        return Ok(bad_sha256.then_some(BadFileReason::Sha256));
    }

    // if we're offline, don't download
    if offline {
        return Ok(None);
    }

    // check last modified
    let res = client
        .head(src)
        .send()
        .await
        .map_err(|e| ReconcileError2::HttpError {
            method: String::from("HEAD"),
            url: src.to_owned(),
            error: e.to_string(),
        })?;

    let Some(last_modified_header) = res
        .headers()
        .get(http::header::LAST_MODIFIED)
        // parse as a string
        .and_then(|e| e.to_str().ok())
    else {
        return Ok(Some(BadFileReason::Stale));
    };

    let Some(remote_content_length) = res
        .headers()
        .get(http::header::CONTENT_LENGTH)
        // parse the header as a u64
        .and_then(|e| e.to_str().ok().and_then(|s| s.parse::<u64>().ok()))
    else {
        return Ok(Some(BadFileReason::Size));
    };

    let remote_last_modified = httpdate::parse_http_date(last_modified_header);
    let local_last_modified = meta
        .modified()
        .map_err(|e| ReconcileError2::FileStatError(dst.to_path_buf(), e.to_string()))?;

    let is_stale = remote_last_modified
        .map(|res| res > local_last_modified)
        .unwrap_or(true);
    Ok(is_stale
        .then_some(BadFileReason::Stale)
        .or_else(|| (remote_content_length != local_content_length).then_some(BadFileReason::Size)))
}
