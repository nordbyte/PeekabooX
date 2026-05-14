fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/peekaboox/v1/peekaboox.proto");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    tonic_prost_build::configure().compile_protos(
        &["../../proto/peekaboox/v1/peekaboox.proto"],
        &["../../proto"],
    )?;

    Ok(())
}
