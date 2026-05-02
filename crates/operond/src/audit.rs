use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use operon_core::{AuditEvent, RequestContext};

use crate::{
    state::{AppState, MAX_IN_MEMORY_AUDIT_EVENTS},
    AUDIT_CONTEXT,
};

pub(crate) fn record_audit(
    state: &AppState,
    action: &str,
    resource: &str,
    allowed: bool,
    reason: &str,
) {
    record_audit_capability(state, "fs:workspace", action, resource, allowed, reason);
}

pub(crate) fn record_audit_capability(
    state: &AppState,
    capability: &str,
    action: &str,
    resource: &str,
    allowed: bool,
    reason: &str,
) {
    let context = current_request_context();
    let event = AuditEvent {
        subject: state.policy.subject.clone(),
        timestamp_ms: now_ms(),
        node_id: state.node.id.clone(),
        capability: capability.to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        allowed,
        reason: reason.to_string(),
        run_id: context.run_id,
        step_id: context.step_id,
    };
    push_audit_event(&state.audit, &state.store_writer, event);
}

pub(crate) fn push_audit_event(
    audit: &Arc<Mutex<VecDeque<AuditEvent>>>,
    store_writer: &operon_store::StoreWriter,
    event: AuditEvent,
) {
    let Ok(mut audit) = audit.lock() else {
        tracing::error!("audit log mutex poisoned");
        return;
    };
    audit.push_back(event.clone());
    while audit.len() > MAX_IN_MEMORY_AUDIT_EVENTS {
        audit.pop_front();
    }
    drop(audit);
    if let Err(error) = append_store_record(
        store_writer,
        &serde_json::json!({
            "kind": "audit",
            "event": event,
        }),
    ) {
        tracing::warn!("failed to persist audit event: {error:#}");
    }
}

pub(crate) fn bounded_audit_events(events: Vec<AuditEvent>) -> VecDeque<AuditEvent> {
    let mut events = VecDeque::from(events);
    while events.len() > MAX_IN_MEMORY_AUDIT_EVENTS {
        events.pop_front();
    }
    events
}

pub(crate) fn current_request_context() -> RequestContext {
    AUDIT_CONTEXT.try_with(Clone::clone).unwrap_or_default()
}

pub(crate) fn append_store_record(
    writer: &operon_store::StoreWriter,
    record: &serde_json::Value,
) -> anyhow::Result<()> {
    writer.append_json_value(record)
}

pub(crate) fn now_ms() -> u64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_audit_events_keeps_recent_persisted_events() {
        let events = (0..MAX_IN_MEMORY_AUDIT_EVENTS + 2)
            .map(|index| AuditEvent {
                subject: "test-subject".to_string(),
                timestamp_ms: index as u64,
                node_id: "node-a".to_string(),
                capability: "fs:workspace".to_string(),
                action: "stat".to_string(),
                resource: format!("/{index}"),
                allowed: true,
                reason: "allowed".to_string(),
                run_id: None,
                step_id: None,
            })
            .collect::<Vec<_>>();

        let bounded = bounded_audit_events(events);

        assert_eq!(bounded.len(), MAX_IN_MEMORY_AUDIT_EVENTS);
        assert_eq!(
            bounded.front().expect("first retained event").resource,
            "/2"
        );
    }
}
