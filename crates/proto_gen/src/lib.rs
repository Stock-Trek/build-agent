use std::path::Path;

/// Generates Rust DTOs from the .proto files in `proto_dir`,
/// writing the resulting `.rs` files into `out_dir`.
pub fn generate(proto_dir: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if out_dir.exists() {
        std::fs::remove_dir_all(out_dir)?;
    }
    std::fs::create_dir_all(out_dir)?;

    let protos: Vec<_> = std::fs::read_dir(proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    prost_build::Config::new()
        .out_dir(out_dir)
        .compile_protos(&protos, &[proto_dir])?;

    let mod_path = out_dir.join("mod.rs");
    let contents = "pub mod dto;\n";
    std::fs::write(mod_path, contents)?;
    Ok(())
}
