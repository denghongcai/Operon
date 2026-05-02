use std::path::PathBuf;

use operon_core::AuditLog;

use crate::{
    grpc,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

#[derive(Debug, Default)]
pub(crate) struct AuditFilter {
    pub(crate) limit: Option<usize>,
    pub(crate) capability: Option<String>,
    pub(crate) action: Option<String>,
    pub(crate) allowed: Option<bool>,
    pub(crate) resource: Option<String>,
}

pub(crate) async fn list(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = grpc::list_audit(&endpoint).await?;
    if output.json {
        print_json(&audit)?;
        return Ok(());
    }
    print(audit, output)
}

pub(crate) async fn show(
    config_path: PathBuf,
    node_id: &str,
    filter: AuditFilter,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;
    let audit: AuditLog = grpc::list_audit(&endpoint).await?;
    let audit = filter_audit(audit, &filter);
    if output.json {
        print_json(&audit)?;
        return Ok(());
    }
    print(audit, output)
}

fn filter_audit(audit: AuditLog, filter: &AuditFilter) -> AuditLog {
    let mut events = audit
        .events
        .into_iter()
        .filter(|event| {
            filter
                .capability
                .as_ref()
                .is_none_or(|capability| &event.capability == capability)
                && filter
                    .action
                    .as_ref()
                    .is_none_or(|action| &event.action == action)
                && filter
                    .allowed
                    .is_none_or(|allowed| event.allowed == allowed)
                && filter
                    .resource
                    .as_ref()
                    .is_none_or(|resource| event.resource.contains(resource))
        })
        .collect::<Vec<_>>();
    if let Some(limit) = filter.limit {
        events = events.into_iter().rev().take(limit).collect::<Vec<_>>();
    }
    AuditLog {
        events,
        next_page_token: audit.next_page_token,
    }
}

fn print(audit: AuditLog, output: OutputMode) -> anyhow::Result<()> {
    if output.quiet {
        return Ok(());
    }
    for event in audit.events {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            event.subject,
            event.timestamp_ms,
            event.node_id,
            event.capability,
            event.action,
            event.resource,
            event.allowed,
            event.reason,
            event.run_id.as_deref().unwrap_or("-"),
            event.step_id.as_deref().unwrap_or("-")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_filter_applies_to_json_and_text_inputs() {
        let audit = AuditLog {
            events: vec![
                test_audit_event("fs:workspace", "stat", true, "/a"),
                test_audit_event("job:default", "run", true, "/"),
                test_audit_event("fs:workspace", "read", false, "/secret"),
            ],
            next_page_token: String::new(),
        };
        let filter = AuditFilter {
            limit: Some(1),
            capability: Some("fs:workspace".to_string()),
            action: None,
            allowed: Some(false),
            resource: Some("secret".to_string()),
        };

        let filtered = filter_audit(audit, &filter);

        assert_eq!(filtered.events.len(), 1);
        assert_eq!(filtered.events[0].action, "read");
    }

    fn test_audit_event(
        capability: &str,
        action: &str,
        allowed: bool,
        resource: &str,
    ) -> operon_core::AuditEvent {
        operon_core::AuditEvent {
            subject: "local-cli".to_string(),
            timestamp_ms: 1,
            node_id: "local".to_string(),
            capability: capability.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            allowed,
            reason: "-".to_string(),
            run_id: None,
            step_id: None,
        }
    }
}
