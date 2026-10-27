//! Linux helper: one PostgreSQL TCP stream per authenticated platform WebSocket.
//! It runs only on the preview's private Docker network, with no published port.
use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{
    Connector,
    tungstenite::{Message, client::IntoClientRequest},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub url: String,
    pub token: String,
    pub ca_pem: Option<String>,
}

fn connector(ca: Option<&str>) -> Result<Connector> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(pem) = ca {
        let mut count = 0;
        for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
            roots.add(cert.context("invalid_tunnel_ca")?)?;
            count += 1;
        }
        ensure!(count > 0, "invalid_tunnel_ca");
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Connector::Rustls(Arc::new(config)))
}

async fn connection(mut tcp: tokio::net::TcpStream, config: &Config, tls: Connector) -> Result<()> {
    let mut request = config.url.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {}", config.token).parse()?);
    let (mut ws, _) = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(tls)),
    )
    .await
    .context("tunnel_connect_timeout")??;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        tokio::select! {
            read = tcp.read(&mut buffer) => {
                let count = read?;
                if count == 0 { let _ = ws.close(None).await; break; }
                ws.send(Message::Binary(buffer[..count].to_vec().into())).await?;
            }
            message = ws.next() => match message {
                Some(Ok(Message::Binary(bytes))) => tcp.write_all(&bytes).await?,
                Some(Ok(Message::Ping(bytes))) => ws.send(Message::Pong(bytes)).await?,
                Some(Ok(Message::Pong(_))) => {},
                Some(Ok(Message::Close(_))) | None => break,
                _ => anyhow::bail!("database_tunnel_closed"),
            }
        }
    }
    let _ = tcp.shutdown().await;
    Ok(())
}

pub async fn run() -> Result<()> {
    let config: Config = serde_json::from_str(
        &std::env::var("MIY_TUNNEL_CONFIG").context("missing_tunnel_configuration")?,
    )?;
    let url = url::Url::parse(&config.url)?;
    ensure!(
        matches!(url.scheme(), "ws" | "wss")
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid_tunnel_url"
    );
    let tls = connector(config.ca_pem.as_deref())?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:5432").await?;
    let permits = Arc::new(tokio::sync::Semaphore::new(16));
    loop {
        let (socket, _) = listener.accept().await?;
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            drop(socket);
            continue;
        };
        let config = config.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // Do not log connection errors: HTTP errors can contain sensitive headers.
            let _ = connection(socket, &config, tls).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_extra_ca_is_rejected() {
        assert!(connector(None).is_ok());
        assert!(connector(Some("not a certificate")).is_err());
    }
}
