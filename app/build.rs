fn main() {
    println!("cargo:rerun-if-changed=icons/icon.rc");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        embed_resource::compile("icons/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
