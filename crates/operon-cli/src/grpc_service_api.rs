use operon_core::{ServiceCheck, ServiceList};
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::{ListServicesRequest, ServiceIdRequest};

use crate::grpc::{call, with_auth, DEFAULT_LIST_PAGE_SIZE};

pub async fn list_services(endpoint: &NodeEndpoint) -> anyhow::Result<ServiceList> {
    let mut services = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_services(with_auth(
                        &endpoint,
                        ListServicesRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        services.extend(
            response
                .services
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(anyhow::Error::msg)?,
        );
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(ServiceList {
        services,
        next_page_token: String::new(),
    })
}

pub async fn check_service(
    endpoint: &NodeEndpoint,
    service_id: &str,
) -> anyhow::Result<ServiceCheck> {
    let service_id = service_id.to_string();
    call(endpoint, |mut client, endpoint| async move {
        Ok(client
            .check_service(with_auth(&endpoint, ServiceIdRequest { service_id })?)
            .await?
            .into_inner()
            .into())
    })
    .await
}
