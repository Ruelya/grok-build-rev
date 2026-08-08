//! Stamp fork builds with a `-rev` version suffix.

fn main() {
    println!("cargo:rerun-if-env-changed=GROK_VERSION");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let base = std::env::var("GROK_VERSION")
        .or_else(|_| std::env::var("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| "0.0.0".to_string());
    let lower = base.to_ascii_lowercase();
    let version = if lower.contains("-rev") || lower.contains("+rev") {
        base
    } else {
        format!("{base}-rev")
    };
    println!("cargo:rustc-env=GROK_VERSION={version}");
}
