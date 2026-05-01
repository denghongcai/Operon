fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/operon/runtime.proto");
    tonic_prost_build::compile_protos("../../proto/operon/runtime.proto")?;
    Ok(())
}
