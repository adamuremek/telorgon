#[cfg(feature = "desktop-wayland-linux")]
#[path = "build/wayland.rs"]
mod wayland;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build");
    println!("cargo:rerun-if-changed=src/wayland_server/protocol.rs");
    #[cfg(feature = "desktop-wayland-linux")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        if let Err(error) = wayland::generate() {
            panic!("Wayland descriptor generation failed: {error}");
        }
    }
}
