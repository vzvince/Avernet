use std::error::Error;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let proto = Path::new("../../../api-contracts/provider-demo/v1/provider_demo.proto");
    let include = Path::new("../../../api-contracts/provider-demo/v1");
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost_config = tonic_prost_build::Config::new();
    prost_config.protoc_executable(protoc);
    tonic_prost_build::configure().compile_with_config(
        prost_config,
        &[proto],
        &[include],
    )?;
    println!("cargo:rerun-if-changed={}", proto.display());
    Ok(())
}
