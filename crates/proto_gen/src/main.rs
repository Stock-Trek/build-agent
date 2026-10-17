use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .ok_or("proto-gen lives under the workspace root")?;
    proto_gen::ProtoGen::generate(&root.join("proto"), &root.join("runner/src/generated"))?;
    Ok(())
}
