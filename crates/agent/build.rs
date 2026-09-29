fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();

    println!("cargo:rerun-if-changed={}", root.join("proto").display());

    proto_gen::generate(&root.join("proto"), &root.join("runner/src/generated"))?;

    Ok(())
}
