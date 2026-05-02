use std::{fs, path::PathBuf};

use operon_core::{ExecutionTrace, TraceFile, TraceFileList};

use crate::output::{print_json, OutputMode};

pub(crate) fn show(path: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?;
    if output.quiet {
        return Ok(());
    }
    if output.json {
        let value: serde_json::Value = serde_json::from_str(&content)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let trace: ExecutionTrace = serde_json::from_str(&content)?;
    println!("{} {} {:?}", trace.run_id, trace.name, trace.status);
    for step in trace.steps {
        println!(
            "{}\t{}\t{}\t{:?}\t{}ms\t{}",
            step.id,
            step.node,
            step.action,
            step.status,
            step.ended_at_ms.saturating_sub(step.started_at_ms),
            step.error.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

pub(crate) fn list(dir: PathBuf, output: OutputMode) -> anyhow::Result<()> {
    let mut traces = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let Ok(trace) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if !(trace.get("run_id").is_some()
            && trace.get("name").is_some()
            && trace.get("steps").is_some())
        {
            continue;
        }
        traces.push(TraceFile {
            path: path.display().to_string(),
            run_id: trace
                .get("run_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            name: trace
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            status: trace
                .get("status")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok()),
        });
    }
    traces.sort_by(|a, b| a.path.cmp(&b.path));
    let list = TraceFileList { traces };
    if output.json {
        print_json(&list)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for trace in list.traces {
        println!(
            "{}\t{}\t{}",
            trace.path,
            trace.run_id.as_deref().unwrap_or("-"),
            trace.name.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}
