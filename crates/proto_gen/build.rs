fn main() {
    prost_build::Config::new()
        .type_attribute(
            ".dto.AssetIdDto",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile_protos(
            &[
                "proto/strategy_context_dto.proto",
                "proto/portfolio_dto.proto",
            ],
            &["proto/"],
        )
        .unwrap();
    println!("cargo::rerun-if-changed=proto/");
    println!("cargo::rerun-if-changed=src/");
}
