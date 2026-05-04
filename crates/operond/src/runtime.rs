use operon_core::{AuditLog, CapabilityList, HealthStatus, ServiceList};
use operon_protocol::runtime::v1::{
    operon_runtime_server::OperonRuntime, CapabilityDiagnosticRequest, ExecIdRequest, FileChunk,
    FsCopyRequest, FsListRequest, FsPathRequest, FsReadRangeRequest, FsRenameRequest,
    FsTruncateRequest, FsWriteRangeRequest, GetNodeRequest, HealthRequest, ListAuditRequest,
    ListCapabilitiesRequest, ListExecsRequest, ListServicesRequest, ServiceDatagramTunnelRequest,
    ServiceIdRequest, ServiceTunnelRequest,
};
use tonic::{Request, Response as GrpcResponse, Status};

use crate::{
    auth::authorize_grpc,
    capability_diagnostics, exec_service, fs_service,
    locks::lock,
    pagination::paginate_items,
    service_forward::{
        self, grpc_service_check, open_service_datagram_tunnel, open_service_tunnel,
    },
    state::AppState,
    AUDIT_CONTEXT,
};

#[derive(Debug, Clone)]
pub(crate) struct GrpcRuntime {
    pub(crate) state: AppState,
}

type GrpcFileStream = fs_service::FileStream;
type GrpcExecLogStream = exec_service::ExecLogStream;
type GrpcExecEventStream = exec_service::ExecEventStream;
type GrpcServiceTunnelStream = service_forward::ServiceTunnelStream;
type GrpcServiceDatagramTunnelStream = service_forward::ServiceDatagramTunnelStream;
type GrpcExecSessionStream = exec_service::ExecSessionStream;

#[tonic::async_trait]
impl OperonRuntime for GrpcRuntime {
    async fn health(
        &self,
        request: Request<HealthRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::HealthStatus>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(
            HealthStatus {
                ok: true,
                node_id: self.state.node.id.clone(),
                version: operon_protocol::PROTOCOL_VERSION.to_string(),
            }
            .into(),
        ))
    }

    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::NodeInfo>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(self.state.node.clone().into()))
    }

    async fn list_capabilities(
        &self,
        request: Request<ListCapabilitiesRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::CapabilityList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (capabilities, next_page_token) = paginate_items(
            &self.state.capabilities.capabilities,
            request.page_size,
            &request.page_token,
        )?;
        let mut response: operon_protocol::runtime::v1::CapabilityList = CapabilityList {
            capabilities,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }

    async fn explain_capability(
        &self,
        request: Request<CapabilityDiagnosticRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::PolicyDecision>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request: operon_core::CapabilityDiagnosticRequest = request.into_inner().into();
        let decision = capability_diagnostics::explain_capability_decision(
            &self.state.policy,
            &self.state.secrets,
            &request,
        );
        Ok(GrpcResponse::new(decision.into()))
    }

    async fn stat_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::stat(&self.state, path).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn list_fs(
        &self,
        request: Request<FsListRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsList>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let list = fs_service::list_page(
                    &self.state,
                    request.path,
                    request.page_size,
                    &request.page_token,
                )
                .await?;
                Ok(GrpcResponse::new(list.into()))
            })
            .await
    }

    type ReadFileStream = GrpcFileStream;

    async fn read_file(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<Self::ReadFileStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = fs_service::read_stream(&self.state, path).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    async fn read_file_range(
        &self,
        request: Request<FsReadRangeRequest>,
    ) -> Result<GrpcResponse<FileChunk>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let chunk =
                    fs_service::read_range(&self.state, request.path, request.offset, request.size)
                        .await?;
                Ok(GrpcResponse::new(chunk))
            })
            .await
    }

    async fn write_file(
        &self,
        request: Request<tonic::Streaming<operon_protocol::runtime::v1::WriteFileRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let write = fs_service::write_stream(&self.state, &mut stream).await?;
                Ok(GrpcResponse::new(write.into()))
            })
            .await
    }

    async fn write_file_range(
        &self,
        request: Request<FsWriteRangeRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsWrite>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (path, offset, data, precondition) =
            fs_service::precondition_from_write_range_request(request);
        AUDIT_CONTEXT
            .scope(context, async {
                let write =
                    fs_service::write_range(&self.state, path, offset, data, precondition).await?;
                Ok(GrpcResponse::new(write.into()))
            })
            .await
    }

    async fn truncate_fs(
        &self,
        request: Request<FsTruncateRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (path, size, precondition) = fs_service::precondition_from_truncate_request(request);
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::truncate(&self.state, path, size, precondition).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn mkdir_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsStat>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let path = request.into_inner().path;
        AUDIT_CONTEXT
            .scope(context, async {
                let stat = fs_service::mkdir(&self.state, path).await?;
                Ok(GrpcResponse::new(stat.into()))
            })
            .await
    }

    async fn delete_fs(
        &self,
        request: Request<FsPathRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsDelete>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let (path, precondition) = fs_service::precondition_from_path_request(request.into_inner());
        AUDIT_CONTEXT
            .scope(context, async {
                let path = fs_service::delete(&self.state, path, precondition).await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsDelete {
                    path,
                }))
            })
            .await
    }

    async fn rename_fs(
        &self,
        request: Request<FsRenameRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsRename>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (from_precondition, to_precondition) =
            fs_service::preconditions_from_rename_request(&request);
        AUDIT_CONTEXT
            .scope(context, async {
                fs_service::rename(
                    &self.state,
                    &request.from_path,
                    &request.to_path,
                    from_precondition,
                    to_precondition,
                )
                .await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsRename {
                    from_path: request.from_path,
                    to_path: request.to_path,
                }))
            })
            .await
    }

    async fn copy_fs(
        &self,
        request: Request<FsCopyRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::FsCopy>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (from_precondition, to_precondition) =
            fs_service::preconditions_from_copy_request(&request);
        AUDIT_CONTEXT
            .scope(context, async {
                let (bytes_copied, version) = fs_service::copy(
                    &self.state,
                    &request.from_path,
                    &request.to_path,
                    from_precondition,
                    to_precondition,
                )
                .await?;
                Ok(GrpcResponse::new(operon_protocol::runtime::v1::FsCopy {
                    from_path: request.from_path,
                    to_path: request.to_path,
                    bytes_copied,
                    version,
                }))
            })
            .await
    }

    async fn run_exec(
        &self,
        request: Request<operon_protocol::runtime::v1::ExecRunRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecRecord>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                Ok(GrpcResponse::new(exec_service::run_exec(
                    &self.state,
                    request,
                )?))
            })
            .await
    }

    async fn get_exec(
        &self,
        request: Request<ExecIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecRecord>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(exec_service::get_exec(
            &self.state,
            request.into_inner(),
        )?))
    }

    async fn list_execs(
        &self,
        request: Request<ListExecsRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        Ok(GrpcResponse::new(exec_service::list_execs(
            &self.state,
            request.into_inner(),
        )?))
    }

    type WatchExecStream = GrpcExecEventStream;

    async fn watch_exec(
        &self,
        request: Request<ExecIdRequest>,
    ) -> Result<GrpcResponse<Self::WatchExecStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let exec_id = request.into_inner().exec_id;
        Ok(GrpcResponse::new(exec_service::watch_exec(
            self.state.clone(),
            exec_id,
        )?))
    }

    async fn list_exec_logs(
        &self,
        request: Request<ExecIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecLogList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let exec_id = request.into_inner().exec_id;
        Ok(GrpcResponse::new(exec_service::list_exec_logs(
            &self.state,
            exec_id,
        )?))
    }

    type StreamExecLogsStream = GrpcExecLogStream;

    async fn stream_exec_logs(
        &self,
        request: Request<ExecIdRequest>,
    ) -> Result<GrpcResponse<Self::StreamExecLogsStream>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let exec_id = request.into_inner().exec_id;
        Ok(GrpcResponse::new(exec_service::stream_exec_logs(
            self.state.clone(),
            exec_id,
        )?))
    }

    async fn write_exec_stdin(
        &self,
        request: Request<tonic::Streaming<operon_protocol::runtime::v1::ExecStdinRequest>>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecStdin>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let mut stream = request.into_inner();
        Ok(GrpcResponse::new(
            exec_service::write_exec_stdin(&self.state, &mut stream).await?,
        ))
    }

    async fn close_exec_stdin(
        &self,
        request: Request<ExecIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecStdinClose>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let exec_id = request.into_inner().exec_id;
        Ok(GrpcResponse::new(exec_service::close_exec_stdin(
            &self.state,
            exec_id,
        )?))
    }

    async fn cancel_exec(
        &self,
        request: Request<operon_protocol::runtime::v1::ExecCancelRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ExecRecord>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let exec_id = request.into_inner().exec_id;
        AUDIT_CONTEXT
            .scope(context, async {
                Ok(GrpcResponse::new(exec_service::cancel_exec(
                    &self.state,
                    exec_id,
                )?))
            })
            .await
    }

    type OpenExecSessionStream = GrpcExecSessionStream;

    async fn open_exec_session(
        &self,
        request: Request<tonic::Streaming<operon_protocol::runtime::v1::ExecSessionRequest>>,
    ) -> Result<GrpcResponse<Self::OpenExecSessionStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let stream = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let output = exec_service::open_exec_session(self.state.clone(), stream).await?;
                Ok(GrpcResponse::new(output))
            })
            .await
    }

    async fn list_services(
        &self,
        request: Request<ListServicesRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceList>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let (services, next_page_token) = paginate_items(
            &self.state.policy.service.services,
            request.page_size,
            &request.page_token,
        )?;
        let mut response: operon_protocol::runtime::v1::ServiceList = ServiceList {
            services,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }

    async fn check_service(
        &self,
        request: Request<ServiceIdRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::ServiceCheck>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let service_id = request.into_inner().service_id;
        AUDIT_CONTEXT
            .scope(context, async {
                let check = grpc_service_check(&self.state, service_id).await?;
                Ok(GrpcResponse::new(check.into()))
            })
            .await
    }

    type OpenServiceTunnelStream = GrpcServiceTunnelStream;

    async fn open_service_tunnel(
        &self,
        request: Request<tonic::Streaming<ServiceTunnelRequest>>,
    ) -> Result<GrpcResponse<Self::OpenServiceTunnelStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let input = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = open_service_tunnel(&self.state, input).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    type OpenServiceDatagramTunnelStream = GrpcServiceDatagramTunnelStream;

    async fn open_service_datagram_tunnel(
        &self,
        request: Request<tonic::Streaming<ServiceDatagramTunnelRequest>>,
    ) -> Result<GrpcResponse<Self::OpenServiceDatagramTunnelStream>, Status> {
        let context = authorize_grpc(&self.state, request.metadata())?;
        let input = request.into_inner();
        AUDIT_CONTEXT
            .scope(context, async {
                let stream = open_service_datagram_tunnel(&self.state, input).await?;
                Ok(GrpcResponse::new(stream))
            })
            .await
    }

    async fn list_audit(
        &self,
        request: Request<ListAuditRequest>,
    ) -> Result<GrpcResponse<operon_protocol::runtime::v1::AuditLog>, Status> {
        authorize_grpc(&self.state, request.metadata())?;
        let request = request.into_inner();
        let events = lock(&self.state.audit, "audit log")?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let (events, next_page_token) =
            paginate_items(&events, request.page_size, &request.page_token)?;
        let mut response: operon_protocol::runtime::v1::AuditLog = AuditLog {
            events,
            next_page_token: String::new(),
        }
        .into();
        response.next_page_token = next_page_token;
        Ok(GrpcResponse::new(response))
    }
}
