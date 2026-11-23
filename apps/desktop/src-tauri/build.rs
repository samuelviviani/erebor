fn main() {
    // Rebuild when the release-time dev fee wallet changes.
    println!("cargo:rerun-if-env-changed=EREBOR_DEV_FEE_WALLET");
    println!("cargo:rerun-if-env-changed=EREBOR_DEV_FEE_BTC");
    println!("cargo:rerun-if-env-changed=EREBOR_REPO_URL");
    tauri_build::build()
}
