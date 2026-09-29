fn main() {
    let proto_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let protos = [
        proto_dir.join("strategy_context_dto.proto"),
        proto_dir.join("portfolio_dto.proto"),
    ];

    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path().unwrap())
        .type_attribute(
            ".dto.AssetIdDto",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile_protos(&protos, &[proto_dir])
        .unwrap();

    println!("cargo::rerun-if-changed=../../proto/");
    println!("cargo::rerun-if-changed=src/");
}
