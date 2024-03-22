use std::path::Path;

use futures::StreamExt;
use http::StatusCode;
use reqwest::IntoUrl;
use tokio::{fs::File, io::AsyncWriteExt};

/// Download a file. Returns a None if 404.
pub async fn download_file(url: impl IntoUrl, to: impl AsRef<Path>) -> anyhow::Result<Option<()>> {
    let req = reqwest::get(url).await?;
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
