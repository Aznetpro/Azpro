use tokio::io::{self, AsyncRead, AsyncWrite};

/// Buffer de relay por direção. 32 KiB melhora transferências contínuas sem
/// reservar buffers excessivos por conexão como os tamanhos muito grandes.
const RELAY_BUFFER_SIZE: usize = 32 * 1024;

pub async fn copy_bidirectional<A, B>(a: &mut A, b: &mut B) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    tokio::io::copy_bidirectional_with_sizes(
        a,
        b,
        RELAY_BUFFER_SIZE,
        RELAY_BUFFER_SIZE,
    )
    .await
}
