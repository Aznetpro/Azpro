use tokio::net::TcpStream;
use std::error::Error;
use log::info;

pub async fn handle_ssh_tunnel(
    mut client_stream: TcpStream,
    target_addr: &str,
) -> Result<(), Box<dyn Error>> {
    info!("🔑 SSH Tunnel para {}", target_addr);
    client_stream.set_nodelay(true)?;

    let mut server_stream = TcpStream::connect(target_addr).await?;
    server_stream.set_nodelay(true)?;
    crate::relay::copy_bidirectional(&mut client_stream, &mut server_stream).await?;

    Ok(())
}
