//! Stable `prompt_cache_key` derivation for main turns and recap.
//!
//! Pure helpers so unit tests need no TUI / HTTP. Main and recap keys are both
//! derived from the session id but are intentionally **not** identical: recap
//! must not collide with the main-turn sticky key (upstream historically set
//! recap to the raw session id; this fork differs by design).

use crate::ApiBackend;

/// Stable main-turn `prompt_cache_key` for a session when auto-attach is enabled.
///
/// Same session id always yields the same main key across turns.
pub fn derive_main_prompt_cache_key(session_id: &str) -> String {
    session_id.to_string()
}

/// Recap `prompt_cache_key`: derived from the session id but never equal to
/// [`derive_main_prompt_cache_key`].
pub fn derive_recap_prompt_cache_key(session_id: &str) -> String {
    format!("xai-recap-{session_id}")
}

/// When `auto_enabled` and the backend forwards the field (Responses), return
/// the main-turn key; otherwise omit (no auto attach).
pub fn resolve_main_auto_prompt_cache_key(
    session_id: &str,
    auto_enabled: bool,
    api_backend: &ApiBackend,
) -> Option<String> {
    if auto_enabled && api_backend.forwards_prompt_cache_key() {
        Some(derive_main_prompt_cache_key(session_id))
    } else {
        None
    }
}

/// Resolve which model slug recap should use.
///
/// Precedence: non-empty `override_model` → when OAuth is present, the official
/// built-in recap model → otherwise the active session model.
pub fn resolve_recap_model_id(
    override_model: Option<&str>,
    oauth_logged_in: bool,
    official_recap_model: &str,
    session_model: &str,
) -> String {
    if let Some(m) = override_model.map(str::trim).filter(|s| !s.is_empty()) {
        return m.to_string();
    }
    if oauth_logged_in && !official_recap_model.is_empty() {
        official_recap_model.to_string()
    } else {
        session_model.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ApiBackend;

    #[test]
    fn main_key_stable_per_session() {
        let a = derive_main_prompt_cache_key("sess-abc");
        let b = derive_main_prompt_cache_key("sess-abc");
        assert_eq!(a, b);
        assert_eq!(a, "sess-abc");
    }

    #[test]
    fn recap_key_derived_but_not_equal_to_main() {
        let session = "sess-xyz";
        let main = derive_main_prompt_cache_key(session);
        let recap = derive_recap_prompt_cache_key(session);
        assert_ne!(main, recap);
        assert!(recap.contains(session));
        assert_eq!(recap, format!("xai-recap-{session}"));
    }

    #[test]
    fn auto_main_key_only_when_enabled_and_responses() {
        let sid = "s1";
        assert_eq!(
            resolve_main_auto_prompt_cache_key(sid, true, &ApiBackend::Responses),
            Some(derive_main_prompt_cache_key(sid))
        );
        assert_eq!(
            resolve_main_auto_prompt_cache_key(sid, true, &ApiBackend::OpenAIResponses),
            Some(derive_main_prompt_cache_key(sid))
        );
        assert_eq!(
            resolve_main_auto_prompt_cache_key(sid, false, &ApiBackend::Responses),
            None
        );
        assert_eq!(
            resolve_main_auto_prompt_cache_key(sid, true, &ApiBackend::ChatCompletions),
            None
        );
        assert_eq!(
            resolve_main_auto_prompt_cache_key(sid, true, &ApiBackend::Messages),
            None
        );
    }

    #[test]
    fn recap_model_override_wins() {
        assert_eq!(
            resolve_recap_model_id(Some("my-recap"), true, "official", "session"),
            "my-recap"
        );
        assert_eq!(
            resolve_recap_model_id(Some("  "), true, "official", "session"),
            "official"
        );
        assert_eq!(
            resolve_recap_model_id(None, true, "official", "session"),
            "official"
        );
        assert_eq!(
            resolve_recap_model_id(None, false, "official", "session"),
            "session"
        );
    }
}
