//! Build-time development asset discovery.

use std::{env, path::PathBuf};

const DEVELOPMENT_VAD_DIR_ENV: &str = "VINPST_SHERPA_VAD_DEVELOPMENT_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={DEVELOPMENT_VAD_DIR_ENV}");

    if env::var_os(DEVELOPMENT_VAD_DIR_ENV).is_some()
        || env::var("PROFILE").is_ok_and(|profile| profile == "release")
    {
        return;
    }

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo should set CARGO_MANIFEST_DIR"),
    );
    let development_vad_dir = manifest_dir.join("../../data/vad");
    println!(
        "cargo:rustc-env={DEVELOPMENT_VAD_DIR_ENV}={}",
        development_vad_dir.display()
    );
}
