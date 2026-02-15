fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::compile_protos("proto/config.proto")?;
    tonic_prost_build::compile_protos("proto/indicators.proto")?;
    tonic_prost_build::compile_protos("proto/depot.proto")?;
    Ok(())
}
