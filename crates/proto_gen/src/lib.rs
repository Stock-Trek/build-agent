use std::path::Path;

/// Generates Rust DTOs from the .proto files in `proto_dir`,
/// writing the resulting `.rs` files into `out_dir`.
pub fn generate(proto_dir: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::remove_dir_all(out_dir)?;
    std::fs::create_dir_all(out_dir)?;

    let protos: Vec<_> = std::fs::read_dir(proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "proto"))
        .collect();

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    prost_build::Config::new()
        .out_dir(out_dir)
        .compile_protos(&protos, &[proto_dir])?;

    Ok(())
}
