fn main() {
    println!("cargo::rerun-if-changed=../../proto/");
    println!("cargo::rerun-if-changed=./");
}
