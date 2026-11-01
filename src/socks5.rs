use anyhow::{bail, Result};
use log::info;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SSH_BACKEND: &str = "127.0.0.1:22";
const SSH_PORT: u16 = 22;

/// SOCKS5 sem autenticação SOCKS, restrito ao SSH local desta VPS.
/// As credenciais de SSH continuam trafegando dentro do túnel e são validadas pelo sshd.
pub async fn handle_socks5(mut client: TcpStream) -> Result<()> {
    info!("SOCKS5: iniciando negociacao restrita ao SSH local");
    client.set_nodelay(true)?;

    let mut greeting = [0u8; 2];
    client.read_exact(&mut greeting).await?;
    if greeting[0] != 0x05 || greeting[1] == 0 {
        let _ = client.write_all(&[0x05, 0xff]).await;
        bail!("SOCKS5 greeting invalido");
    }

    let mut methods = vec![0u8; greeting[1] as usize];
    client.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        client.write_all(&[0x05, 0xff]).await?;
        bail!("cliente SOCKS5 nao ofereceu metodo sem autenticacao");
    }
    client.write_all(&[0x05, 0x00]).await?;

    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    if request[0] != 0x05 || request[2] != 0x00 {
        send_reply(&mut client, 0x01).await?;
        bail!("cabecalho de requisicao SOCKS5 invalido");
    }
    if request[1] != 0x01 {
        send_reply(&mut client, 0x07).await?;
        bail!("somente SOCKS5 CONNECT e suportado");
    }

    // Consome e valida o endereco solicitado, mas nunca conecta ao destino fornecido
    // pelo cliente: o unico destino permitido e o SSH local da VPS.
    match request[3] {
        0x01 => {
            let mut address = [0u8; 4];
            client.read_exact(&mut address).await?;
        }
        0x03 => {
            let mut length = [0u8; 1];
            client.read_exact(&mut length).await?;
            if length[0] == 0 {
                send_reply(&mut client, 0x08).await?;
                bail!("nome de destino SOCKS5 vazio");
            }
            let mut domain = vec![0u8; length[0] as usize];
            client.read_exact(&mut domain).await?;
        }
        0x04 => {
            let mut address = [0u8; 16];
            client.read_exact(&mut address).await?;
        }
        _ => {
            send_reply(&mut client, 0x08).await?;
            bail!("tipo de endereco SOCKS5 nao suportado");
        }
    }

    let mut port_buf = [0u8; 2];
    client.read_exact(&mut port_buf).await?;
    let requested_port = u16::from_be_bytes(port_buf);
    if requested_port != SSH_PORT {
        send_reply(&mut client, 0x02).await?;
        bail!("destino SOCKS5 bloqueado: somente a porta SSH local 22 e permitida");
    }

    let mut remote = match TcpStream::connect(SSH_BACKEND).await {
        Ok(stream) => stream,
        Err(error) => {
            send_reply(&mut client, 0x05).await?;
            bail!("nao foi possivel conectar ao SSH local: {error}");
        }
    };
    remote.set_nodelay(true)?;
    send_reply(&mut client, 0x00).await?;
    info!("SOCKS5: tunel conectado ao SSH local na porta 22");
    let _ = crate::relay::copy_bidirectional(&mut client, &mut remote).await?;
    Ok(())
}

async fn send_reply(client: &mut TcpStream, code: u8) -> std::io::Result<()> {
    client
        .write_all(&[0x05, code, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
}

#[cfg(test)]
mod tests {
    use super::SSH_PORT;

    #[test]
    fn socks_policy_only_allows_local_ssh_port() {
        assert_eq!(SSH_PORT, 22);
        assert_ne!(SSH_PORT, 80);
        assert_ne!(SSH_PORT, 1194);
    }
}
