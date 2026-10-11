use base64::Engine;
use bytes::{Buf, BufMut, Bytes};
use napi::{bindgen_prelude::Buffer, Error, Result};
use napi_derive::napi;
use operon_core::NodeEndpoint;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tonic::{
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    metadata::MetadataMap,
    Request, Status,
};

#[derive(Clone, Default)]
struct RawCodec;
impl Codec for RawCodec {
    type Encode = Bytes;
    type Decode = Bytes;
    type Encoder = Self;
    type Decoder = Self;
    fn encoder(&mut self) -> Self {
        Self
    }
    fn decoder(&mut self) -> Self {
        Self
    }
}
impl Encoder for RawCodec {
    type Item = Bytes;
    type Error = Status;
    fn encode(&mut self, item: Bytes, dst: &mut EncodeBuf<'_>) -> std::result::Result<(), Status> {
        dst.put(item);
        Ok(())
    }
}
impl Decoder for RawCodec {
    type Item = Bytes;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> std::result::Result<Option<Bytes>, Status> {
        Ok(Some(src.copy_to_bytes(src.remaining())))
    }
}

fn failure(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}

fn serialize_metadata(metadata: &MetadataMap) -> std::result::Result<String, Status> {
    let pairs: Vec<_> = metadata
        .iter()
        .map(|entry| match entry {
            tonic::metadata::KeyAndValueRef::Ascii(key, value) => (
                key.as_str(),
                String::from_utf8_lossy(value.as_encoded_bytes()).into_owned(),
                false,
            ),
            tonic::metadata::KeyAndValueRef::Binary(key, value) => (
                key.as_str(),
                String::from_utf8_lossy(value.as_encoded_bytes()).into_owned(),
                true,
            ),
        })
        .collect();
    serde_json::to_string(&pairs).map_err(|_| Status::internal("metadata serialization failed"))
}

#[napi]
pub struct NativeClient {
    endpoint: NodeEndpoint,
    channel: Mutex<Option<tonic::transport::Channel>>,
    connecting: tokio::sync::Mutex<()>,
    cancelled: CancellationToken,
}

#[napi]
impl NativeClient {
    #[napi(constructor)]
    pub fn new(config_json: String) -> Result<Self> {
        let endpoint: NodeEndpoint = serde_json::from_str(&config_json).map_err(failure)?;
        if !endpoint.endpoint.starts_with("iroh://") {
            return Err(failure(
                "native SDK backend requires explicit iroh selection",
            ));
        }
        Ok(Self {
            endpoint,
            channel: Mutex::new(None),
            connecting: tokio::sync::Mutex::new(()),
            cancelled: CancellationToken::new(),
        })
    }

    #[napi(factory)]
    pub async fn connect(config_json: String) -> Result<Self> {
        let client = Self::new(config_json)?;
        client.ready().await?;
        Ok(client)
    }

    #[napi]
    pub async fn ready(&self) -> Result<()> {
        let _connecting = tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => return Err(failure("native client is closed")),
            lock = self.connecting.lock() => lock,
        };
        if self
            .channel
            .lock()
            .map_err(|_| failure("client channel lock poisoned"))?
            .is_some()
        {
            return Ok(());
        }
        let channel = tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => return Err(failure("native client is closed")),
            channel = operon_grpc_client::connect_channel(&self.endpoint,
                Duration::from_secs(self.endpoint.transport.connect_timeout_secs)) => channel.map_err(failure)?,
        };
        let mut stored = self
            .channel
            .lock()
            .map_err(|_| failure("client channel lock poisoned"))?;
        if self.cancelled.is_cancelled() {
            return Err(failure("native client is closed"));
        }
        *stored = Some(channel);
        Ok(())
    }

    #[napi]
    pub async fn open_call(&self, path: String, metadata_json: String) -> Result<NativeCall> {
        if self.cancelled.is_cancelled() {
            return Err(failure("native client is closed"));
        }
        if !path.starts_with("/operon.runtime.v1.OperonRuntime/") {
            return Err(failure("invalid Operon runtime RPC path"));
        }
        self.ready().await?;
        let path: http::uri::PathAndQuery = path.parse().map_err(failure)?;
        let values: Vec<(String, String)> =
            serde_json::from_str(&metadata_json).map_err(failure)?;
        let mut metadata = MetadataMap::new();
        for (key, value) in values {
            if key.ends_with("-bin") {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .map_err(failure)?;
                metadata.append_bin(
                    key.parse::<tonic::metadata::MetadataKey<tonic::metadata::Binary>>()
                        .map_err(failure)?,
                    tonic::metadata::MetadataValue::from_bytes(&bytes),
                );
                continue;
            }
            metadata.append(
                key.parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>()
                    .map_err(failure)?,
                value.parse().map_err(failure)?,
            );
        }
        let (send, requests) = mpsc::channel(4);
        let (responses, receive) = mpsc::channel(4);
        let (header_send, header_receive) = tokio::sync::oneshot::channel();
        let trailers = Arc::new(Mutex::new(None));
        let received_trailers = trailers.clone();
        let cancelled = self.cancelled.child_token();
        let stop = cancelled.clone();
        let channel = self
            .channel
            .lock()
            .map_err(|_| failure("client channel lock poisoned"))?
            .clone()
            .ok_or_else(|| failure("native client is closed"))?;
        let mut grpc = tonic::client::Grpc::new(channel)
            .max_decoding_message_size(operon_protocol::MAX_GRPC_MESSAGE_BYTES)
            .max_encoding_message_size(operon_protocol::MAX_GRPC_MESSAGE_BYTES);
        let task = tokio::spawn(async move {
            let mut header_send = Some(header_send);
            let operation = async {
                grpc.ready()
                    .await
                    .map_err(|error| Status::unavailable(error.to_string()))?;
                let mut request =
                    Request::new(tokio_stream::wrappers::ReceiverStream::new(requests));
                *request.metadata_mut() = metadata;
                let response = grpc.streaming(request, path, RawCodec).await?;
                if let Some(header_send) = header_send.take() {
                    let _ = header_send.send(serialize_metadata(response.metadata()));
                }
                let mut stream = response.into_inner();
                while let Some(message) = stream.message().await? {
                    if responses.send(Ok(message)).await.is_err() {
                        return Ok(());
                    }
                }
                if let Some(metadata) = stream.trailers().await? {
                    *received_trailers
                        .lock()
                        .map_err(|_| Status::internal("trailer lock poisoned"))? =
                        Some(serialize_metadata(&metadata)?);
                }
                Ok::<_, Status>(())
            };
            tokio::select! {
                biased;
                _ = stop.cancelled() => {},
                result = operation => {
                    if let Err(error) = result {
                        if let Some(header_send) = header_send.take() { let _ = header_send.send(Err(error.clone())); }
                        let _ = tokio::select! { _ = stop.cancelled() => Ok(()), result = responses.send(Err(error)) => result };
                    }
                },
            }
        });
        Ok(NativeCall {
            send: tokio::sync::Mutex::new(Some(send)),
            receive: tokio::sync::Mutex::new(receive),
            headers: tokio::sync::Mutex::new(Some(header_receive)),
            trailers,
            cancelled,
            task: Mutex::new(Some(task)),
        })
    }

    #[napi]
    pub fn close(&self) {
        self.cancelled.cancel();
        if let Ok(mut channel) = self.channel.lock() {
            channel.take();
        }
    }
}

impl Drop for NativeClient {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}

#[napi]
pub struct NativeCall {
    send: tokio::sync::Mutex<Option<mpsc::Sender<Bytes>>>,
    receive: tokio::sync::Mutex<mpsc::Receiver<std::result::Result<Bytes, Status>>>,
    headers: tokio::sync::Mutex<
        Option<tokio::sync::oneshot::Receiver<std::result::Result<String, Status>>>,
    >,
    trailers: Arc<Mutex<Option<String>>>,
    cancelled: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}

#[napi]
impl NativeCall {
    #[napi]
    pub async fn headers(&self) -> Result<String> {
        let receiver = self
            .headers
            .lock()
            .await
            .take()
            .ok_or_else(|| failure("headers already consumed"))?;
        tokio::select! { biased;
            _ = self.cancelled.cancelled() => Err(failure("call cancelled")),
            metadata = receiver => metadata.map_err(failure)?.map_err(|status| failure(format!("grpc-status:{}:{}", status.code() as i32, status.message()))),
        }
    }
    #[napi]
    pub fn trailers(&self) -> Result<String> {
        Ok(self
            .trailers
            .lock()
            .map_err(|_| failure("trailer lock poisoned"))?
            .clone()
            .unwrap_or_else(|| "[]".into()))
    }
    #[napi]
    pub async fn send(&self, message: Buffer) -> Result<()> {
        if message.len() > operon_protocol::MAX_GRPC_MESSAGE_BYTES {
            return Err(failure("RPC message exceeds transport limit"));
        }
        let owned = Bytes::copy_from_slice(&message);
        let sender = self.send.lock().await;
        let sender = sender
            .as_ref()
            .ok_or_else(|| failure("request stream already finished"))?;
        tokio::select! { biased; _ = self.cancelled.cancelled() => Err(failure("call cancelled")),
        result = sender.send(owned) => result.map_err(|_| failure("request stream closed")) }
    }
    #[napi]
    pub async fn finish_send(&self) {
        self.send.lock().await.take();
    }
    #[napi]
    pub async fn receive(&self) -> Result<Option<Buffer>> {
        let mut receive = self.receive.lock().await;
        tokio::select! { biased; _ = self.cancelled.cancelled() => Err(failure("call cancelled")),
        message = receive.recv() => match message {
            Some(Ok(bytes)) => Ok(Some(bytes.to_vec().into())),
            Some(Err(status)) => Err(failure(format!("grpc-status:{}:{}", status.code() as i32, status.message()))),
            None => Ok(None),
        } }
    }
    #[napi]
    pub fn cancel(&self) {
        self.cancelled.cancel();
    }
    #[napi]
    pub async fn close(&self) -> Result<()> {
        self.cancelled.cancel();
        let task = self
            .task
            .lock()
            .map_err(|_| failure("call task lock poisoned"))?
            .take();
        if let Some(task) = task {
            task.await.map_err(failure)?;
        }
        Ok(())
    }
}
impl Drop for NativeCall {
    fn drop(&mut self) {
        self.cancelled.cancel();
        if let Ok(mut task) = self.task.lock() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}
