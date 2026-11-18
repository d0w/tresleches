use std::io;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

const MAX_FRAME: usize = 64 * 1024;

pub(super) async fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    // 4 byte header is agreed upon via spec
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;

    let size = u32::from_be_bytes(header) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame size",
        ));
    }

    let mut body = vec![0u8; size];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

pub(super) async fn write_frame(stream: &mut UnixStream, body: &[u8]) -> io::Result<()> {
    if body.is_empty() || body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid body size",
        ));
    }

    // write | big endian len(body) | body |
    stream.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stream.write_all(body).await
}
