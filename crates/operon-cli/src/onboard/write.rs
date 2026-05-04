use super::GeneratedFile;

pub(super) fn write_plan_files(files: &[GeneratedFile]) -> anyhow::Result<()> {
    for file in files {
        write_generated_file(file)?;
    }
    Ok(())
}

pub(super) fn write_generated_file(file: &GeneratedFile) -> anyhow::Result<()> {
    super::write_generated_file(file)
}
