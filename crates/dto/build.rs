fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .ancestors()
        .nth(2)
        .expect("dto crate should live in the crates directory");
    let proto_dir = root.join("proto");
    println!("cargo::rerun-if-changed={}", proto_dir.display());
    proto_gen::generate(&proto_dir, &manifest_dir.join("src/generated"))?;
    Ok(())
}
