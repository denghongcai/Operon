fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=poc.proto");
    println!("cargo:rerun-if-changed=../../proto/operon/runtime.proto");
    tonic_prost_build::configure().compile_protos(
        &["poc.proto", "../../proto/operon/runtime.proto"],
        &[".", "../../proto"],
    )?;
    Ok(())
}
