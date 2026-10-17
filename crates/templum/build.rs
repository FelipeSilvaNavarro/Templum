//! Windows takes a program's icon from its resources: Explorer, the taskbar, the shortcuts the
//! installers make, Add or remove programs, and the title bar (GPUI loads icon 1). Elsewhere the
//! installers carry the icons (`[package.metadata.packager]` in Cargo.toml).

fn main() {
    // Without these, Cargo runs this again whenever any file of the crate changes.
    println!("cargo:rerun-if-changed=icons/templum.rc");
    println!("cargo:rerun-if-changed=icons/templum.ico");
    #[cfg(windows)]
    embed_resource::compile(
        "icons/templum.rc",
        embed_resource::ParamsIncludeDirs(["icons"]),
    )
    .manifest_optional()
    .unwrap();
}
