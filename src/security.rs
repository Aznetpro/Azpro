use anyhow::{bail, Result};
use log::info;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const MAX_SECURITY_PREFACE: usize = 8192;

/// Handler do modo SECURITY histórico do projeto: status 101, leitura do preâmbulo,
/// status 200 com Upgrade: security e túnel para SSH/VPN local.
pub async fn handle_security(mut socket: TcpStream, status: &str, ssh_only: bool) -> Result<()> {
    socket.set_nodelay(true)?;
    socket
        .write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes())
        .await?;
    socket.flush().await?;

    let mut preface = vec![0u8; MAX_SECURITY_PREFACE];
    let bytes_read = match timeout(Duration::from_secs(5), socket.read(&mut preface)).await {
        Ok(Ok(n)) if n > 0 => n,
        Ok(Ok(_)) => bail!("cliente fechou durante o handshake SECURITY"),
        Ok(Err(error)) => return Err(error.into()),
        Err(_) => bail!("timeout lendo preâmbulo SECURITY"),
    };
    let preface_text = String::from_utf8_lossy(&preface[..bytes_read]);
    let preferred = if ssh_only || preface_text.contains("SSH") || preface_text.contains("ssh") {
        "127.0.0.1:22"
    } else {
        "127.0.0.1:1194"
    };
    let alternate = if preferred == "127.0.0.1:22" {
        "127.0.0.1:1194"
    } else {
        "127.0.0.1:22"
    };

    let mut backend = match timeout(Duration::from_secs(3), TcpStream::connect(preferred)).await {
        Ok(Ok(stream)) => stream,
        _ if !ssh_only => {
            match timeout(Duration::from_secs(3), TcpStream::connect(alternate)).await {
                Ok(Ok(stream)) => stream,
                _ => bail!("backends SECURITY SSH/VPN indisponiveis"),
            }
        }
        _ => bail!("backend SECURITY SSH indisponivel em 127.0.0.1:22"),
    };
    backend.set_nodelay(true)?;

    let response = format!(
        "HTTP/1.1 200 {}\r\nConnection: Upgrade\r\nUpgrade: security\r\n\r\n",
        status
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    info!(
        "SECURITY conectado ao backend {} (preâmbulo {} bytes)",
        preferred, bytes_read
    );

    // O preâmbulo é controle do handshake SECURITY, como no handler histórico;
    // somente os dados subsequentes entram no túnel.
    let _ = crate::relay::copy_bidirectional(&mut socket, &mut backend).await?;
    Ok(())
}
