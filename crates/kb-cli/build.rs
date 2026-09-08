use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=KB_RELEASE_TARGET");
    println!("cargo:rerun-if-changed=../../assets/release/kb-update.minisign.pub");

    let Ok(target) = env::var("KB_RELEASE_TARGET") else {
        return;
    };
    if !matches!(
        target.as_str(),
        "x86_64-unknown-linux-gnu" | "aarch64-apple-darwin" | "x86_64-pc-windows-msvc"
    ) {
        panic!("KB_RELEASE_TARGET is not a supported release target: {target}");
    }

    let key_path = PathBuf::from("../../assets/release/kb-update.minisign.pub");
    let key = fs::read_to_string(&key_path).unwrap_or_else(|error| {
        panic!(
            "official release builds require {}: {error}",
            key_path.display()
        )
    });
    let encoded_key = key
        .lines()
        .nth(1)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "official release builds require a two-line Minisign public key at {}",
                key_path.display()
            )
        });
    println!("cargo:rustc-env=KB_RELEASE_TARGET={target}");
    println!("cargo:rustc-env=KB_RELEASE_PUBLIC_KEY={encoded_key}");
}
