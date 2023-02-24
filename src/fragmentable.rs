use byteorder::{BigEndian, ByteOrder};
use futures_util::{future::LocalBoxFuture, FutureExt};
use icepipe::pipe_stream::{Control, PipeStream, StreamError, WaitThen};

const MAX_LEN: usize = 4096;

pub struct Fragmentable<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    underlying: P,
    rx_buf: Vec<u8>,
}
impl<P> Fragmentable<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    pub fn new(underlying: P) -> Self {
        Self {
            underlying,
            rx_buf: Default::default(),
        }
    }

    async fn send_packet(&mut self, mut packet: &[u8]) -> Result<(), P::Error> {
        while !packet.is_empty() {
            let n = MAX_LEN.min(packet.len());
            let send = &packet[..n];
            packet = &packet[n..];
            self.underlying.send(send).await?;
        }

        Ok(())
    }

    fn read_ready(&self) -> bool {
        if self.rx_buf.len() < 4 {
            return false;
        }

        self.rx_buf.len() - 4 >= self.next_packet_len()
    }

    fn next_packet_len(&self) -> usize {
        BigEndian::read_u32(&self.rx_buf) as usize
    }

    fn consume(&mut self) -> Vec<u8> {
        let total_n = self.next_packet_len();
        let out = self.rx_buf[4..][..total_n].to_vec();
        self.rx_buf = self.rx_buf[4..][total_n..].to_vec();
        out
    }
}
impl<P> PipeStream for Fragmentable<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    fn send<'a>(&'a mut self, data: &'a [u8]) -> LocalBoxFuture<'a, Result<(), P::Error>> {
        async move {
            let mut packet = vec![0; 4];
            BigEndian::write_u32(&mut packet, data.len() as u32);
            packet.extend(data);
            self.send_packet(&packet).await?;

            Ok(())
        }
        .boxed_local()
    }
}
impl<P> Control for Fragmentable<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    fn close(&mut self) -> LocalBoxFuture<'_, Result<(), P::Error>> {
        Control::close(&mut self.underlying)
    }

    fn rx_closed(&self) -> bool {
        Control::rx_closed(&self.underlying)
    }
}
impl<P> WaitThen for Fragmentable<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    type Value = Option<P::Value>;
    type Output = Option<Vec<u8>>;
    type Error = P::Error;

    fn wait(&mut self) -> LocalBoxFuture<'_, Result<Self::Value, P::Error>> {
        async move {
            if self.read_ready() {
                return Ok(None);
            }

            Ok(Some(WaitThen::wait(&mut self.underlying).await?))
        }
        .boxed_local()
    }

    fn then<'a>(
        &'a mut self,
        value: &'a mut Self::Value,
    ) -> LocalBoxFuture<'_, Result<Self::Output, P::Error>> {
        async move {
            let value = value.take();
            if let Some(mut value) = value {
                if let Some(data) = self.underlying.then(&mut value).await? {
                    self.rx_buf.extend(data);
                }
            }

            if self.read_ready() {
                return Ok(Some(self.consume()));
            }

            Ok(None)
        }
        .boxed_local()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use futures_util::FutureExt;
    use rstest::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    #[derive(Default, Clone)]
    struct ArcStream(Arc<Mutex<VecDeque<Vec<u8>>>>);
    impl PipeStream for ArcStream {
        fn send<'a>(&'a mut self, data: &'a [u8]) -> LocalBoxFuture<'a, Result<(), StreamError>> {
            async move {
                self.0.lock().unwrap().push_back(data.to_vec());
                Ok(())
            }
            .boxed_local()
        }
    }
    impl Control for ArcStream {
        fn close(&mut self) -> LocalBoxFuture<'_, Result<(), StreamError>> {
            unreachable!()
        }

        fn rx_closed(&self) -> bool {
            unreachable!()
        }
    }
    impl WaitThen for ArcStream {
        type Value = ();
        type Output = Option<Vec<u8>>;
        type Error = StreamError;

        fn wait(&mut self) -> LocalBoxFuture<'_, Result<(), StreamError>> {
            async move {
                let empty = self.0.lock().unwrap().is_empty();
                if empty {
                    std::future::pending::<()>().await;
                    unreachable!()
                };

                Ok(())
            }
            .boxed_local()
        }

        fn then<'a>(
            &'a mut self,
            _value: &'a mut (),
        ) -> LocalBoxFuture<'_, Result<Self::Output, StreamError>> {
            async move { Ok(self.0.lock().unwrap().pop_front()) }.boxed_local()
        }
    }

    #[rstest]
    #[tokio::test]
    async fn send_fragments_receive_defragments() {
        let stream = ArcStream::default();
        let mut fragmentable = Fragmentable::new(stream.clone());

        let data = (0..6000).map(|i| (i % 256) as u8).collect::<Vec<_>>();
        fragmentable.send(&data).await.unwrap();

        {
            let mut total = vec![];
            let mut buffer = stream.0.lock().unwrap();
            assert_eq!(buffer.len(), 2);
            assert_eq!(buffer[0].len(), 4096);
            assert_eq!(buffer[1].len(), 6000 - 4096 + 2);
            total.extend(buffer[0].iter().copied());
            total.extend(buffer[1].iter().copied());
            buffer.clear();

            for byte in total {
                buffer.push_back(vec![byte]);
            }
        }

        let data_back = loop {
            let mut value = fragmentable.wait().await.unwrap();
            if let Some(data_back) = fragmentable.then(&mut value).await.unwrap() {
                break data_back;
            }
        };
        assert_eq!(data_back, data);
    }

    #[rstest]
    #[tokio::test]
    async fn receiving_two_packets_in_one_recv() {
        let stream = ArcStream(Arc::new(Mutex::new(
            [vec![0, 1, 10, 0, 1, 11]].into_iter().collect(),
        )));
        let mut fragmentable = Fragmentable::new(stream.clone());

        let mut value = fragmentable.wait().await.unwrap();
        let data = fragmentable.then(&mut value).await.unwrap();
        assert_eq!(data, Some(vec![10]));
        let mut value = fragmentable.wait().await.unwrap();
        let data = fragmentable.then(&mut value).await.unwrap();
        assert_eq!(data, Some(vec![11]));
    }
}
