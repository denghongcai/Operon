use std::path::PathBuf;

use operon_core::{CapabilityDiagnosticRequest, CapabilityList, PolicyDecision};

use crate::{
    grpc,
    output::{print_json, OutputMode},
    target::load_endpoint,
};

pub(crate) async fn list(
    config_path: PathBuf,
    node_id: &str,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;

    let list: CapabilityList = grpc::list_capabilities(&endpoint).await?;
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    for capability in list.capabilities {
        println!(
            "{}/{}\t{:?}\t{}",
            capability.node_id,
            capability.id,
            capability.kind,
            capability.permissions.join(",")
        );
    }

    Ok(())
}

pub(crate) async fn explain(
    config_path: PathBuf,
    node_id: &str,
    request: CapabilityDiagnosticRequest,
    output: OutputMode,
) -> anyhow::Result<()> {
    let endpoint = load_endpoint(config_path, node_id)?;

    let decision = grpc::explain_capability(&endpoint, request).await?;
    if output.json {
        print_json(&decision)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }

    print_policy_decision(&decision);
    Ok(())
}

fn print_policy_decision(decision: &PolicyDecision) {
    println!(
        "{} {} {} allowed={} reason={} subject={} message={}",
        decision.capability_id,
        decision.action,
        decision.resource,
        decision.allowed,
        decision.reason_code.as_str(),
        decision.subject,
        decision.message
    );
}
