use futures::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p::{
    core::upgrade::{read_length_prefixed, write_length_prefixed},
    request_response::{ProtocolName, RequestResponseCodec},
};
use std::io;

#[derive(Debug, Clone)]
pub struct Protocol;
#[derive(Clone)]
pub struct Codec;

impl ProtocolName for Protocol {
    fn protocol_name(&self) -> &[u8] {
        "/file-exchange/1".as_bytes()
    }
}

#[async_trait::async_trait]
impl RequestResponseCodec for Codec {
    type Protocol = Protocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T>(&mut self, _: &Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(vec)
    }

    async fn read_response<T>(&mut self, _: &Protocol, io: &mut T) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 500_000_000).await?; // update transfer maximum

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(vec)
    }

    async fn write_request<T>(
        &mut self,
        _: &Protocol,
        io: &mut T,
        data: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Protocol,
        io: &mut T,
        data: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }
}
