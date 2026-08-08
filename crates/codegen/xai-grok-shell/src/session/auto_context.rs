//! Reserved Auto Context injection hook (experimental — not implemented).
//!
//! Research targets: Augment Code / Devin-style automatic context selection.
//! Until implemented, every public API here is a **no-op** so default session
//! construction is unchanged when no Auto Context config is present.

/// Experimental config surface. All fields currently unused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoContextConfig {
    /// Master switch. Default `false` — feature off.
    pub enabled: bool,
}

/// Whether Auto Context would inject anything for this session.
///
/// Always `false` while the feature is a stub (even if `enabled` is set).
pub fn should_inject(_cfg: &AutoContextConfig) -> bool {
    false
}

/// Reserved injection point: would return extra system/user context items.
///
/// Always returns an empty list (no-op).
pub fn inject_context_items(_cfg: &AutoContextConfig) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_noop() {
        let cfg = AutoContextConfig::default();
        assert!(!cfg.enabled);
        assert!(!should_inject(&cfg));
        assert!(inject_context_items(&cfg).is_empty());
    }

    #[test]
    fn enabled_still_noop_until_implemented() {
        let cfg = AutoContextConfig { enabled: true };
        assert!(!should_inject(&cfg), "stub must not inject even when enabled");
        assert!(inject_context_items(&cfg).is_empty());
    }
}
