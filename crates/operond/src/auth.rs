use operon_core::RequestContext;
use tonic::{metadata::MetadataMap, Status};

use crate::state::AppState;

const RUN_ID_METADATA: &str = "x-operon-run-id";
const STEP_ID_METADATA: &str = "x-operon-step-id";

pub(crate) fn authorize_grpc(
    state: &AppState,
    metadata: &MetadataMap,
) -> Result<RequestContext, Status> {
    if let Some(expected) = &state.auth_token {
        let Some(header) = metadata.get("authorization") else {
            return Err(Status::unauthenticated("missing bearer token"));
        };
        let Ok(header) = header.to_str() else {
            return Err(Status::unauthenticated("invalid bearer token"));
        };
        let Some(actual) = header.strip_prefix("Bearer ") else {
            return Err(Status::unauthenticated("invalid bearer token"));
        };
        if actual != expected {
            return Err(Status::unauthenticated("invalid bearer token"));
        }
    }

    Ok(RequestContext {
        run_id: metadata_value(metadata, RUN_ID_METADATA),
        step_id: metadata_value(metadata, STEP_ID_METADATA),
    })
}

fn metadata_value(metadata: &MetadataMap, key: &'static str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
