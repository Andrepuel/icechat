use byteorder::{BigEndian, ByteOrder};
use futures_util::FutureExt;
use icepipe::{
    pipe_stream::{Control, PipeStream, WaitThen, WaitThenDyn},
    DynResult,
};
use std::any::Any;

const MAX_LEN: usize = 4096;

pub struct Fragmentable<P: PipeStream> {
    underlying: P,
    rx_buf: Vec<u8>,
}
impl<P: PipeStream> Fragmentable<P> {
    pub fn new(underlying: P) -> Self {
        Self {
            underlying,
            rx_buf: Default::default(),
        }
    }

    async fn send_packet(&mut self, mut packet: &[u8]) -> DynResult<()> {
        while !packet.is_empty() {
            let n = MAX_LEN.min(packet.len());
            let send = &packet[..n];
            packet = &packet[n..];
            self.underlying.send(send).await?;
        }

        Ok(())
    }

    fn read_ready(&self) -> bool {
        if self.rx_buf.len() < 2 {
            return false;
        }

        self.rx_buf.len() - 2 >= self.next_packet_len()
    }

    fn next_packet_len(&self) -> usize {
        BigEndian::read_u16(&self.rx_buf) as usize
    }

    fn consume(&mut self) -> Vec<u8> {
        let total_n = self.next_packet_len();
        let out = self.rx_buf[2..][..total_n].to_vec();
        self.rx_buf = self.rx_buf[2..][total_n..].to_vec();
        out
    }
}
impl<P: PipeStream> PipeStream for Fragmentable<P> {
    fn send<'a>(&'a mut self, data: &'a [u8]) -> icepipe::PinFutureLocal<'a, ()> {
        async move {
            let mut packet = vec![0; 2];
            BigEndian::write_u16(&mut packet, data.len() as u16);
            packet.extend(data);
            self.send_packet(&packet).await?;

            Ok(())
        }
        .boxed_local()
    }
}
impl<P: PipeStream> Control for Fragmentable<P> {
    fn close(&mut self) -> icepipe::PinFutureLocal<'_, ()> {
        Control::close(&mut self.underlying)
    }

    fn rx_closed(&self) -> bool {
        Control::rx_closed(&self.underlying)
    }
}
impl<P: PipeStream> WaitThen for Fragmentable<P> {
    type Value = Option<Box<dyn Any>>;
    type Output = Option<Vec<u8>>;

    fn wait(&mut self) -> icepipe::PinFutureLocal<'_, Option<Box<dyn Any>>> {
        async move {
            if self.read_ready() {
                return Ok(None);
            }

            Ok(Some(WaitThenDyn::wait_dyn(&mut self.underlying).await?))
        }
        .boxed_local()
    }

    fn then<'a>(
        &'a mut self,
        value: &'a mut Option<Box<dyn Any>>,
    ) -> icepipe::PinFutureLocal<'_, Self::Output> {
        async move {
            let value = value.take();
            if let Some(mut value) = value {
                if let Some(data) = self.underlying.then_dyn(&mut value).await? {
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
        fn send<'a>(&'a mut self, data: &'a [u8]) -> icepipe::PinFutureLocal<'a, ()> {
            async move {
                self.0.lock().unwrap().push_back(data.to_vec());
                Ok(())
            }
            .boxed_local()
        }
    }
    impl Control for ArcStream {
        fn close(&mut self) -> icepipe::PinFutureLocal<'_, ()> {
            unreachable!()
        }

        fn rx_closed(&self) -> bool {
            unreachable!()
        }
    }
    impl WaitThenDyn for ArcStream {
        type Output = Option<Vec<u8>>;

        fn wait_dyn(&mut self) -> icepipe::PinFutureLocal<'_, Box<dyn Any>> {
            async move {
                let empty = self.0.lock().unwrap().is_empty();
                if empty {
                    std::future::pending::<()>().await;
                    unreachable!()
                };

                let r: Box<dyn Any> = Box::new(());

                Ok(r)
            }
            .boxed_local()
        }

        fn then_dyn<'a>(
            &'a mut self,
            _value: &'a mut Box<dyn std::any::Any>,
        ) -> icepipe::PinFutureLocal<'_, Self::Output> {
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
            let mut value = fragmentable.wait_dyn().await.unwrap();
            if let Some(data_back) = fragmentable.then_dyn(&mut value).await.unwrap() {
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

        let mut value = fragmentable.wait_dyn().await.unwrap();
        let data = fragmentable.then_dyn(&mut value).await.unwrap();
        assert_eq!(data, Some(vec![10]));
        let mut value = fragmentable.wait_dyn().await.unwrap();
        let data = fragmentable.then_dyn(&mut value).await.unwrap();
        assert_eq!(data, Some(vec![11]));
    }
}
