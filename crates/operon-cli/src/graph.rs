use std::{path::PathBuf, time::SystemTime};

use operon_core::{
    ExecutionGraph, ExecutionStatus, ExecutionStep, ExecutionStepTrace, ExecutionTrace, FsRead,
    JobRecord, JobRunRequest, JobStatus, RequestContext,
};

use crate::{commands::job::load as load_job, grpc, target::load_endpoint};

pub(crate) async fn run_graph(
    config_path: PathBuf,
    workflow_path: PathBuf,
    trace_output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let workflow = std::fs::read_to_string(&workflow_path)?;
    let graph: ExecutionGraph = serde_yaml::from_str(&workflow)?;
    let mut trace = ExecutionTrace {
        run_id: format!("run-{}", now_ms()),
        name: graph.name.clone(),
        status: ExecutionStatus::Running,
        steps: Vec::new(),
    };

    for (index, step) in graph.steps.iter().enumerate() {
        let step_trace = execute_step(config_path.clone(), &trace.run_id, index, step).await;
        let failed = matches!(step_trace.status, ExecutionStatus::Failed);
        trace.steps.push(step_trace);

        if failed {
            trace.status = ExecutionStatus::Failed;
            write_trace(&trace, trace_output.as_deref())?;
            return Err(anyhow::anyhow!("execution graph `{}` failed", graph.name));
        }
    }

    trace.status = ExecutionStatus::Succeeded;
    write_trace(&trace, trace_output.as_deref())
}

async fn execute_step(
    config_path: PathBuf,
    run_id: &str,
    index: usize,
    step: &ExecutionStep,
) -> ExecutionStepTrace {
    let id = step
        .id
        .clone()
        .unwrap_or_else(|| format!("step-{}", index + 1));
    let started_at_ms = now_ms();
    let result = grpc::with_request_context(
        RequestContext {
            run_id: Some(run_id.to_string()),
            step_id: Some(id.clone()),
        },
        || execute_step_action(config_path, step),
    )
    .await;
    let ended_at_ms = now_ms();

    match result {
        Ok(output) => ExecutionStepTrace {
            id,
            node: step.node.clone(),
            action: step.action.clone(),
            status: ExecutionStatus::Succeeded,
            started_at_ms,
            ended_at_ms,
            error: None,
            output: Some(output),
        },
        Err(error) => ExecutionStepTrace {
            id,
            node: step.node.clone(),
            action: step.action.clone(),
            status: ExecutionStatus::Failed,
            started_at_ms,
            ended_at_ms,
            error: Some(error.to_string()),
            output: None,
        },
    }
}

async fn execute_step_action(
    config_path: PathBuf,
    step: &ExecutionStep,
) -> anyhow::Result<serde_json::Value> {
    match step.action.as_str() {
        "fs.stat" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let stat = grpc::fs_stat(&endpoint, path).await?;
            Ok(serde_json::to_value(stat)?)
        }
        "fs.list" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let list = grpc::fs_list(&endpoint, path).await?;
            Ok(serde_json::to_value(list)?)
        }
        "fs.read" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let mut content = Vec::new();
            grpc::read_file_to_writer(&endpoint, path, &mut content).await?;
            let read = FsRead {
                path: path.to_string(),
                content: String::from_utf8(content)?,
            };
            Ok(serde_json::to_value(read)?)
        }
        "fs.write" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let content = step.content.clone().unwrap_or_default();
            let write = grpc::write_file_bytes(&endpoint, path, content.as_bytes()).await?;
            Ok(serde_json::to_value(write)?)
        }
        "job.run" => run_job_step(config_path, step).await,
        action => anyhow::bail!("unsupported graph action `{action}`"),
    }
}

async fn run_job_step(
    config_path: PathBuf,
    step: &ExecutionStep,
) -> anyhow::Result<serde_json::Value> {
    let endpoint = load_endpoint(config_path.clone(), &step.node)?;
    let request = JobRunRequest {
        command: required_field(step.command.as_deref(), "command")?.to_string(),
        cwd: step.cwd.clone(),
        timeout_secs: step.timeout_secs,
        secrets: step.secrets.clone(),
    };
    let record: JobRecord = grpc::run_job(&endpoint, request).await?;

    let event = grpc::watch_job_to_terminal(&endpoint, &record.id).await?;
    let record = load_job(config_path, &step.node, &record.id).await?;
    match event.status {
        JobStatus::Succeeded => Ok(serde_json::to_value(record)?),
        JobStatus::Running => anyhow::bail!("job `{}` watch ended while still running", record.id),
        JobStatus::Failed | JobStatus::Cancelled | JobStatus::TimedOut => {
            anyhow::bail!("job `{}` ended with status {:?}", record.id, event.status)
        }
    }
}

fn required_field<'a>(value: Option<&'a str>, field: &str) -> anyhow::Result<&'a str> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("step requires `{field}`"))
}

fn write_trace(trace: &ExecutionTrace, output: Option<&std::path::Path>) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(trace)?;
    if let Some(output) = output {
        std::fs::write(output, &content)?;
    }
    println!("{content}");
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_field_accepts_non_empty_value() {
        assert_eq!(
            required_field(Some("/workspace"), "path").expect("value"),
            "/workspace"
        );
    }

    #[test]
    fn required_field_rejects_empty_value() {
        let error = required_field(Some(""), "command").expect_err("empty should fail");
        assert_eq!(error.to_string(), "step requires `command`");
    }

    #[tokio::test]
    async fn execute_step_reports_unsupported_action_as_failed_trace() {
        let step = ExecutionStep {
            id: Some("bad-step".to_string()),
            node: "node-a".to_string(),
            action: "screen.read".to_string(),
            path: None,
            content: None,
            command: None,
            cwd: None,
            timeout_secs: None,
            secrets: Vec::new(),
        };

        let trace = execute_step(PathBuf::from("missing-config.yaml"), "run-test", 0, &step).await;

        assert_eq!(trace.id, "bad-step");
        assert!(matches!(trace.status, ExecutionStatus::Failed));
        assert_eq!(
            trace.error.as_deref(),
            Some("unsupported graph action `screen.read`")
        );
    }

    #[tokio::test]
    async fn execute_step_generates_default_step_id() {
        let step = ExecutionStep {
            id: None,
            node: "node-a".to_string(),
            action: "unsupported".to_string(),
            path: None,
            content: None,
            command: None,
            cwd: None,
            timeout_secs: None,
            secrets: Vec::new(),
        };

        let trace = execute_step(PathBuf::from("missing-config.yaml"), "run-test", 2, &step).await;

        assert_eq!(trace.id, "step-3");
    }
}
