fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let proto_dir = root.join("proto");
    println!("cargo::rerun-if-changed={}", proto_dir.display());
    proto_gen::generate(&proto_dir, &root.join("runner/src/generated"))?;
    Ok(())
}
