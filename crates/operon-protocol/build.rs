fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/operon/runtime.proto");
    tonic_prost_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(&["../../proto/operon/runtime.proto"], &["../../proto"])?;
    Ok(())
}
