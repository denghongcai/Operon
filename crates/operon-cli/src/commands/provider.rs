use operon_config::NetworkProviderKind;

use crate::output::{print_json, OutputMode};

pub(crate) fn list(output: OutputMode) -> anyhow::Result<()> {
    let providers: Vec<_> = NetworkProviderKind::all()
        .iter()
        .map(NetworkProviderKind::as_str)
        .collect();
    if output.json {
        print_json(&providers)?;
        return Ok(());
    }
    if output.quiet {
        return Ok(());
    }
    for provider in providers {
        println!("{provider}");
    }
    Ok(())
}
