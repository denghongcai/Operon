use super::*;
use operon_protocol::runtime::v1::{write_file_request, WriteFileRequest, WriteFileTarget};

struct RunningServer(tokio::task::JoinHandle<Result<(), tonic::transport::Error>>);
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn grpc_range_limits_content_and_failed_stream_contract() {
    let dir = tempfile::tempdir().unwrap();
    let mut policy = crate::defaults::default_policy();
    policy.fs.mounts = vec![operon_core::FsMountPolicy {
        name: "workspace".into(),
        path: "/".into(),
        permissions: operon_core::FsPermissions {
            read: true,
            write: true,
            delete: true,
        },
    }];
    let state = crate::daemon_state::test_state(policy, dir.path().into());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = futures_util::stream::unfold(listener, |listener| async {
        Some((listener.accept().await.map(|(socket, _)| socket), listener))
    });
    let _server = RunningServer(tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(GrpcRuntime { state }.into_service())
            .serve_with_incoming(incoming),
    ));
    let endpoint = operon_core::runtime::NodeEndpoint {
        transport: Default::default(),
        node_id: "local".into(),
        endpoint: format!("grpc://{address}"),
        token: None,
    };
    let mut client = operon_grpc_client::connect(&endpoint).await.unwrap();
    let data = (0..operon_protocol::MAX_FS_DATA_BYTES)
        .map(|i| (i % 251) as u8)
        .collect::<Vec<_>>();
    let write = client
        .write_file_range(FsWriteRangeRequest {
            path: "/large".into(),
            offset: 37,
            data: data.clone(),
            precondition: None,
            expected_version: None,
            require_absent: false,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(write.stat.as_ref().unwrap().size, data.len() as u64 + 37);
    for size in [0, 2 * 1024 * 1024 + 1, 4 * 1024 * 1024 + 1, data.len()] {
        let read = client
            .read_file_range(FsReadRangeRequest {
                path: "/large".into(),
                offset: 37,
                size: size as u32,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(read.data, data[..size]);
    }
    for offset in [
        data.len() as u64 + 30,
        data.len() as u64 + 37,
        data.len() as u64 + 99,
    ] {
        let read = client
            .read_file_range(FsReadRangeRequest {
                path: "/large".into(),
                offset,
                size: 100,
            })
            .await
            .unwrap()
            .into_inner();
        let remaining = (data.len() as u64 + 37).saturating_sub(offset) as usize;
        assert_eq!(read.data, data[data.len() - remaining..]);
    }
    for (offset, size) in [(0, data.len() as u32 + 1), (u64::MAX, 1)] {
        assert_eq!(
            client
                .read_file_range(FsReadRangeRequest {
                    path: "/large".into(),
                    offset,
                    size
                })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
    let oversized = client
        .write_file_range(FsWriteRangeRequest {
            path: "/large".into(),
            offset: 0,
            data: vec![0; data.len() + 1],
            precondition: None,
            expected_version: None,
            require_absent: false,
        })
        .await
        .unwrap_err();
    assert_eq!(oversized.code(), tonic::Code::InvalidArgument);
    // A peer without the generated client's limit still cannot send an
    // encoded message above the configured server receive bound.
    let channel = operon_grpc_client::connect_channel(&endpoint, std::time::Duration::from_secs(2))
        .await
        .unwrap();
    let mut unrestricted =
        operon_protocol::runtime::v1::operon_runtime_client::OperonRuntimeClient::new(channel);
    let error = unrestricted
        .write_file_range(FsWriteRangeRequest {
            path: "/large".into(),
            offset: 0,
            data: vec![0; operon_protocol::MAX_GRPC_MESSAGE_BYTES + 1],
            precondition: None,
            expected_version: None,
            require_absent: false,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::OutOfRange);
    std::fs::write(dir.path().join("atomic"), b"ORIGINAL").unwrap();
    let stream = futures_util::stream::iter([
        WriteFileRequest {
            payload: Some(write_file_request::Payload::Target(WriteFileTarget {
                path: "/atomic".into(),
                precondition: None,
                expected_version: None,
                require_absent: false,
            })),
        },
        WriteFileRequest {
            payload: Some(write_file_request::Payload::Chunk(FileChunk {
                data: b"NEW".to_vec(),
            })),
        },
        WriteFileRequest { payload: None },
    ]);
    assert_eq!(
        client.write_file(stream).await.unwrap_err().code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(
        std::fs::read(dir.path().join("atomic")).unwrap(),
        b"ORIGINAL"
    );
}
