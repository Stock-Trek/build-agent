fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    proto_gen::generate(&root.join("proto"), &root.join("runner/src/generated"))?;
    Ok(())
}
