use async_trait::async_trait;
use std::io::Result;

/// iAP2 传输层抽象。
///
/// 所有方法均为异步，允许底层使用 tokio、blocking thread 或任何 IO 模型。
/// 实现者必须保证 `Send + Sync + Unpin + 'static`，以便在 tokio runtime 中安全调度。
#[async_trait]
pub trait Iap2Transport: Send + Sync + Unpin + 'static {
    /// 读取最多 `buf.len()` 字节，返回实际读取的字节数。
    /// 返回 0 表示连接关闭。
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// 精确读取 `buf.len()` 字节。如果连接在中途关闭则返回 `UnexpectedEof`。
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// 写入全部字节。
    async fn write_all(&mut self, buf: &[u8]) -> Result<()>;

    /// 刷新底层写缓冲区。
    async fn flush(&mut self) -> Result<()>;
}

// ─────────────────────────────────────────────────
// FakeTransport：用于单元测试和集成测试
// ─────────────────────────────────────────────────

#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    use super::*;
    use std::collections::VecDeque;
    use std::io::{Error, ErrorKind};
    use std::sync::{Arc, Mutex};

    /// FakeTransport 内部共享状态。
    /// `rx_queue` 模拟"设备发来的数据"，`tx_log` 记录"发送给设备的数据"。
    #[derive(Debug, Default)]
    pub struct FakeTransportState {
        /// 按顺序注入要读取的字节块
        pub rx_queue: VecDeque<Vec<u8>>,
        /// 记录所有写出的字节
        pub tx_log: Vec<Vec<u8>>,
        /// 如果 true，下次 read 返回 EOF
        pub closed: bool,
    }

    /// 可用于测试的 iAP2 传输。
    ///
    /// # 用法
    /// ```ignore
    /// use iap2_rs::transport::fake::FakeTransport;
    ///
    /// let (transport, state) = FakeTransport::new();
    /// // 注入 iPhone 响应
    /// state.lock().unwrap().rx_queue.push_back(vec![0xFF, 0x5A, 0x00, 0x06, 0xEE, 0x10]);
    /// // 运行被测代码，然后检查发送日志
    /// let sent = state.lock().unwrap().tx_log.clone();
    /// ```
    pub struct FakeTransport {
        state: Arc<Mutex<FakeTransportState>>,
        read_buffer: Vec<u8>,
    }

    impl FakeTransport {
        pub fn new() -> (Self, Arc<Mutex<FakeTransportState>>) {
            let state = Arc::new(Mutex::new(FakeTransportState::default()));
            let transport = Self {
                state: state.clone(),
                read_buffer: Vec::new(),
            };
            (transport, state)
        }

        /// 便捷构造：预置好读取数据
        pub fn with_rx_data(chunks: Vec<Vec<u8>>) -> (Self, Arc<Mutex<FakeTransportState>>) {
            let (transport, state) = Self::new();
            {
                let mut s = state.lock().unwrap();
                for chunk in chunks {
                    s.rx_queue.push_back(chunk);
                }
            }
            (transport, state)
        }
    }

    #[async_trait]
    impl Iap2Transport for FakeTransport {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            // 先耗尽内部 buffer
            if !self.read_buffer.is_empty() {
                let len = std::cmp::min(buf.len(), self.read_buffer.len());
                buf[..len].copy_from_slice(&self.read_buffer[..len]);
                self.read_buffer.drain(..len);
                return Ok(len);
            }

            let mut state = self.state.lock().unwrap();
            if state.closed {
                return Ok(0);
            }
            match state.rx_queue.pop_front() {
                Some(data) => {
                    drop(state);
                    let len = std::cmp::min(buf.len(), data.len());
                    buf[..len].copy_from_slice(&data[..len]);
                    if len < data.len() {
                        self.read_buffer = data[len..].to_vec();
                    }
                    Ok(len)
                }
                None => {
                    // 没有更多数据，模拟 EOF
                    Ok(0)
                }
            }
        }

        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize> {
            let mut offset = 0;
            while offset < buf.len() {
                let n = self.read(&mut buf[offset..]).await?;
                if n == 0 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "FakeTransport EOF"));
                }
                offset += n;
            }
            Ok(offset)
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            if state.closed {
                return Err(Error::new(ErrorKind::BrokenPipe, "FakeTransport closed"));
            }
            state.tx_log.push(buf.to_vec());
            Ok(())
        }

        async fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }
}

// ─────────────────────────────────────────────────
// Linux BlueZ / bluer RFCOMM transport (feature-gated)
// ─────────────────────────────────────────────────

#[cfg(feature = "linux-bluer")]
mod bluer_transport {
    use super::*;

    #[async_trait]
    impl Iap2Transport for bluer::rfcomm::Stream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            tokio::io::AsyncReadExt::read(self, buf).await
        }

        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize> {
            tokio::io::AsyncReadExt::read_exact(self, buf).await
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
            tokio::io::AsyncWriteExt::write_all(self, buf).await
        }

        async fn flush(&mut self) -> Result<()> {
            tokio::io::AsyncWriteExt::flush(self).await
        }
    }
}
