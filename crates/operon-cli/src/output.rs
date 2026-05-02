#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputMode {
    pub(crate) json: bool,
    pub(crate) quiet: bool,
}

pub(crate) fn print_json(value: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
