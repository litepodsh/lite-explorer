fn main() {
    // tauri-build embeds icons/icon.ico in the Windows executable but only re-runs when
    // tauri.conf.json changes, so a new icon would otherwise keep the old one in cached builds.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
