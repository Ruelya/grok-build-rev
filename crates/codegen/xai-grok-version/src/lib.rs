//! Installed grok CLI version, lockstepped with shipping binaries.

use semver::Version;

pub const TEST_VERSION_ENV: &str = "GROK_TEST_VERSION";

/// Product version string. Fork builds use a `-rev` suffix (e.g. `0.2.121-rev`).
pub const VERSION: &str = match option_env!("GROK_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// True when this binary was built as grok-build-rev (version contains `-rev`).
pub fn is_fork_build() -> bool {
    let v = VERSION.to_ascii_lowercase();
    v.contains("-rev") || v.contains("+rev")
}

/// [`TEST_VERSION_ENV`] override first, then [`VERSION`].
pub fn installed() -> String {
    std::env::var(TEST_VERSION_ENV)
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|_| VERSION.to_string())
}

pub fn installed_semver() -> Result<Version, semver::Error> {
    Version::parse(&installed())
}

/// Format the compiled version with a channel label for user-facing display.
///
/// `channel_label` is a pre-formatted suffix such as `" [alpha]"`, `" [stable]"`,
/// or `""`. Obtain it from `xai_grok_update::channel_label()`.
pub fn display_version(channel_label: &str) -> String {
    format!("{}{}", VERSION, channel_label)
}

/// Format a version-with-commit string with a channel label.
pub fn display_version_with_commit(version_with_commit: &str, channel_label: &str) -> String {
    format!("{}{}", version_with_commit, channel_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_version_formatting_matrix() {
        let cases: &[(&str, &str, &str)] = &[
            ("0.2.5 (abc1234)", " [alpha]", "0.2.5 (abc1234) [alpha]"),
            ("0.2.5 (abc1234)", " [stable]", "0.2.5 (abc1234) [stable]"),
            ("0.2.5 (abc1234)", "", "0.2.5 (abc1234)"),
        ];
        for (vwc, label, expected) in cases {
            assert_eq!(display_version_with_commit(vwc, label), *expected);
        }
        assert_eq!(display_version(""), VERSION);
        assert!(display_version(" [stable]").ends_with("[stable]"));
    }
}
