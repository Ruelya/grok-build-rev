use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=GROK_VERSION");

    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let base = std::env::var("GROK_VERSION")
        .or_else(|_| std::env::var("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| "0.0.0".to_string());
    let lower = base.to_ascii_lowercase();
    let version = if lower.contains("-rev") || lower.contains("+rev") {
        base
    } else {
        format!("{base}-rev")
    };

    // Example: "0.2.121-rev (079b9cb)"
    println!("cargo:rustc-env=VERSION_WITH_COMMIT={version} ({commit})");
    if std::env::var_os("GROK_VERSION").is_none() {
        println!("cargo:rustc-env=GROK_VERSION={version}");
    }
}
