use operon_core::AuditLog;
use operon_network::NodeEndpoint;
use operon_protocol::runtime::v1::ListAuditRequest;

use crate::grpc::{call, with_auth, DEFAULT_LIST_PAGE_SIZE};

pub async fn list_audit(endpoint: &NodeEndpoint) -> anyhow::Result<AuditLog> {
    let mut events = Vec::new();
    let mut page_token = String::new();
    loop {
        let response = call(endpoint, |mut client, endpoint| {
            let page_token = page_token.clone();
            async move {
                Ok(client
                    .list_audit(with_auth(
                        &endpoint,
                        ListAuditRequest {
                            page_size: DEFAULT_LIST_PAGE_SIZE,
                            page_token,
                        },
                    )?)
                    .await?
                    .into_inner())
            }
        })
        .await?;
        events.extend(response.events.into_iter().map(Into::into));
        if response.next_page_token.is_empty() {
            break;
        }
        page_token = response.next_page_token;
    }
    Ok(AuditLog {
        events,
        next_page_token: String::new(),
    })
}
