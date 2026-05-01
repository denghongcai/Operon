use std::{path::PathBuf, time::SystemTime};

use operon_core::{
    ExecutionGraph, ExecutionStatus, ExecutionStep, ExecutionStepTrace, ExecutionTrace, FsList,
    FsRead, FsStat, FsWrite, FsWriteRequest, JobRecord, JobRunRequest, JobStatus,
};

use crate::{encode_path, http_get_json, http_post_json, load_endpoint, load_job};

pub(crate) fn run_graph(config_path: PathBuf, workflow_path: PathBuf) -> anyhow::Result<()> {
    let workflow = std::fs::read_to_string(&workflow_path)?;
    let graph: ExecutionGraph = serde_yaml::from_str(&workflow)?;
    let mut trace = ExecutionTrace {
        run_id: format!("run-{}", now_ms()),
        name: graph.name.clone(),
        status: ExecutionStatus::Running,
        steps: Vec::new(),
    };

    for (index, step) in graph.steps.iter().enumerate() {
        let step_trace = execute_step(config_path.clone(), index, step);
        let failed = matches!(step_trace.status, ExecutionStatus::Failed);
        trace.steps.push(step_trace);

        if failed {
            trace.status = ExecutionStatus::Failed;
            print_trace(&trace)?;
            return Err(anyhow::anyhow!("execution graph `{}` failed", graph.name));
        }
    }

    trace.status = ExecutionStatus::Succeeded;
    print_trace(&trace)
}

fn execute_step(config_path: PathBuf, index: usize, step: &ExecutionStep) -> ExecutionStepTrace {
    let id = step
        .id
        .clone()
        .unwrap_or_else(|| format!("step-{}", index + 1));
    let started_at_ms = now_ms();
    let result = execute_step_action(config_path, step);
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

fn execute_step_action(
    config_path: PathBuf,
    step: &ExecutionStep,
) -> anyhow::Result<serde_json::Value> {
    match step.action.as_str() {
        "fs.stat" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let stat: FsStat = http_get_json(
                &endpoint.endpoint,
                &format!("/fs/stat?path={}", encode_path(path)),
            )?;
            Ok(serde_json::to_value(stat)?)
        }
        "fs.list" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let list: FsList = http_get_json(
                &endpoint.endpoint,
                &format!("/fs/list?path={}", encode_path(path)),
            )?;
            Ok(serde_json::to_value(list)?)
        }
        "fs.read" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let read: FsRead = http_get_json(
                &endpoint.endpoint,
                &format!("/fs/read?path={}", encode_path(path)),
            )?;
            Ok(serde_json::to_value(read)?)
        }
        "fs.write" => {
            let endpoint = load_endpoint(config_path, &step.node)?;
            let path = required_field(step.path.as_deref(), "path")?;
            let request = FsWriteRequest {
                path: path.to_string(),
                content: step.content.clone().unwrap_or_default(),
            };
            let write: FsWrite = http_post_json(&endpoint.endpoint, "/fs/write", &request)?;
            Ok(serde_json::to_value(write)?)
        }
        "job.run" => run_job_step(config_path, step),
        action => anyhow::bail!("unsupported graph action `{action}`"),
    }
}

fn run_job_step(config_path: PathBuf, step: &ExecutionStep) -> anyhow::Result<serde_json::Value> {
    let endpoint = load_endpoint(config_path.clone(), &step.node)?;
    let request = JobRunRequest {
        command: required_field(step.command.as_deref(), "command")?.to_string(),
        cwd: step.cwd.clone(),
        timeout_secs: step.timeout_secs,
    };
    let record: JobRecord = http_post_json(&endpoint.endpoint, "/job/run", &request)?;

    loop {
        let record = load_job(config_path.clone(), &step.node, &record.id)?;
        match record.status {
            JobStatus::Running => std::thread::sleep(std::time::Duration::from_millis(100)),
            JobStatus::Succeeded => return Ok(serde_json::to_value(record)?),
            JobStatus::Failed | JobStatus::Cancelled | JobStatus::TimedOut => {
                anyhow::bail!("job `{}` ended with status {:?}", record.id, record.status)
            }
        }
    }
}

fn required_field<'a>(value: Option<&'a str>, field: &str) -> anyhow::Result<&'a str> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("step requires `{field}`"))
}

fn print_trace(trace: &ExecutionTrace) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(trace)?);
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
