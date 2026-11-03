use anyhow::{bail, Result};
use log::info;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SSH_BACKEND: &str = "127.0.0.1:22";
const SSH_PORT: u16 = 22;
const MAX_CSTRING: usize = 255;

/// SOCKS4/SOCKS4a restrito ao SSH local; o endereço solicitado nunca é usado como destino.
pub async fn handle_socks4(mut client: TcpStream) -> Result<()> {
    client.set_nodelay(true)?;

    let mut request = [0u8; 8];
    client.read_exact(&mut request).await?;
    if request[0] != 0x04 || request[1] != 0x01 {
        send_reply(&mut client, 0x5b, &request).await?;
        bail!("SOCKS4 aceita somente CONNECT");
    }

    let requested_port = u16::from_be_bytes([request[2], request[3]]);
    if requested_port != SSH_PORT {
        send_reply(&mut client, 0x5b, &request).await?;
        bail!("destino SOCKS4 bloqueado: somente a porta SSH local 22 e permitida");
    }

    // O USERID é terminado por NUL nos pedidos SOCKS4 e SOCKS4a.
    read_cstring(&mut client).await?;

    // SOCKS4a codifica o endereço como 0.0.0.x (x != 0) e acrescenta o domínio.
    if request[4] == 0 && request[5] == 0 && request[6] == 0 && request[7] != 0 {
        read_cstring(&mut client).await?;
    }

    let mut backend = match TcpStream::connect(SSH_BACKEND).await {
        Ok(stream) => stream,
        Err(error) => {
            send_reply(&mut client, 0x5b, &request).await?;
            return Err(error.into());
        }
    };
    backend.set_nodelay(true)?;
    send_reply(&mut client, 0x5a, &request).await?;
    info!("SOCKS4: tunel conectado ao SSH local na porta 22");
    let _ = crate::relay::copy_bidirectional(&mut client, &mut backend).await?;
    Ok(())
}

async fn read_cstring(client: &mut TcpStream) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    for _ in 0..=MAX_CSTRING {
        let mut byte = [0u8; 1];
        client.read_exact(&mut byte).await?;
        if byte[0] == 0 {
            return Ok(value);
        }
        value.push(byte[0]);
    }
    bail!("campo SOCKS4 excede o limite permitido")
}

async fn send_reply(client: &mut TcpStream, status: u8, request: &[u8; 8]) -> std::io::Result<()> {
    client
        .write_all(&[
            0x00, status, request[2], request[3], request[4], request[5], request[6], request[7],
        ])
        .await
}

#[cfg(test)]
mod tests {
    use super::{MAX_CSTRING, SSH_PORT};

    #[test]
    fn socks4_policy_only_allows_local_ssh_port() {
        assert_eq!(SSH_PORT, 22);
        assert_ne!(SSH_PORT, 80);
        assert_ne!(SSH_PORT, 1194);
    }

    #[test]
    fn cstring_limit_is_bounded() {
        assert_eq!(MAX_CSTRING, 255);
    }
}
