//! DeepSeek Harness *Anchored Standard* phase logic.
//!
//! Port of `xiaobright/dsh-anchored-standard` (a DeepSeek Harness plugin)
//! onto grok-build-rev. Phase is derived from durable conversation items so
//! resume and reload preserve it — the same contract as the plugin's
//! `compaction-epoch.mjs` + `tool-bootstrap.mjs`.
//!
//! Phases:
//! - **Bootstrap** — first model request: official Minimal pair only
//!   (`bash` + `str_replace_editor`), no auto-injected AGENTS.md / skill catalog.
//! - **Promoted** — after the first assistant message (with or without tool
//!   calls) past the last compaction boundary: resident catalog
//!   (bootstrap pair + discovery tools + names unlocked via `dev_tool_search`).
//! - **CompactionRecovery** — after a compaction, before a *new* promotion
//!   signal: bootstrap pair plus a small mid-task work set.

use std::collections::HashSet;

use xai_grok_sampling_types::{ConversationItem, SyntheticReason};

/// Built-in agent type name (`BuiltinAgentName::DshAnchoredStandard`).
pub const AGENT_NAME: &str = "dsh-anchored-standard";

/// Official DeepSeek Harness Minimal persona (`complete: true`).
pub const PERSONA: &str = "You are a helpful software engineer assistant.";

/// Marker embedded in the one-shot post-promotion instruction hint.
pub const INSTRUCTION_HINT_MARKER: &str = "[dsh-instruction-hint]";

/// Project-root instruction files probed for the hint (DSH order).
pub const PROJECT_INSTRUCTION_CANDIDATES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "AGENTS.local.md",
    "CLAUDE.local.md",
];

/// Official Minimal first-request pair (issue #11: the decisive schema).
pub const BOOTSTRAP_TOOLS: &[&str] = &["bash", "str_replace_editor"];

/// Always resident after promotion (Claude tool-search pattern).
pub const DISCOVERY_TOOLS: &[&str] = &["dev_tool_search", "skill_search", "skill_load"];

/// Mid-task work set after compaction, before re-promotion.
///
/// DSH default: `read, write, edit, glob, grep, todo_write, ask_user_question`.
/// Mapped onto grok-build-rev client-facing names.
pub const COMPACTION_TOOLS: &[&str] = &[
    "read_file",
    "write",
    "search_replace",
    "list_dir",
    "grep",
    "todo_write",
    "ask_user_question",
];

/// Session phase for the anchored-standard catalog filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshPhase {
    /// Request #1 of an epoch: Minimal pair only.
    Bootstrap,
    /// Post-compaction, waiting for a new promotion signal.
    CompactionRecovery,
    /// Resident catalog (bootstrap + discovery + unlocked).
    Promoted,
}

/// Derive the phase from durable conversation items.
///
/// Last `SyntheticReason::CompactionMeta` is the epoch boundary (DSH
/// `compaction/end`). An `Assistant` item after that boundary is the
/// `promoteOn: either` signal (first `tool/call` or `assistant/message`).
/// Subagents are treated as already promoted (`includeSubagents: false`).
pub fn derive_phase(conversation: &[ConversationItem], is_subagent: bool) -> DshPhase {
    if is_subagent {
        return DshPhase::Promoted;
    }
    let mut last_compaction = None;
    for (index, item) in conversation.iter().enumerate() {
        if let ConversationItem::User(user) = item
            && user.synthetic_reason == Some(SyntheticReason::CompactionMeta)
        {
            last_compaction = Some(index);
        }
    }
    let start = last_compaction.map(|index| index + 1).unwrap_or(0);
    let promoted = conversation[start..]
        .iter()
        .any(|item| matches!(item, ConversationItem::Assistant(_)));
    if promoted {
        DshPhase::Promoted
    } else if last_compaction.is_some() {
        DshPhase::CompactionRecovery
    } else {
        DshPhase::Bootstrap
    }
}

/// Tool names the model explicitly unlocked via `dev_tool_search.toolNames`.
///
/// Read from durable assistant tool-call arguments so resume keeps them.
pub fn unlocked_tool_names(conversation: &[ConversationItem]) -> HashSet<String> {
    let mut unlocked = HashSet::new();
    for item in conversation {
        let ConversationItem::Assistant(assistant) = item else {
            continue;
        };
        for call in &assistant.tool_calls {
            if call.name != "dev_tool_search" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(call.arguments.as_ref())
            else {
                continue;
            };
            let Some(names) = value.get("toolNames").and_then(|v| v.as_array()) else {
                continue;
            };
            for name in names {
                if let Some(name) = name.as_str()
                    && !name.is_empty()
                {
                    unlocked.insert(name.to_string());
                }
            }
        }
    }
    unlocked
}

/// Keep-set for the current phase (client-facing tool names).
pub fn keep_tool_names(conversation: &[ConversationItem], is_subagent: bool) -> HashSet<String> {
    let mut keep: HashSet<String> = BOOTSTRAP_TOOLS.iter().map(|s| (*s).to_string()).collect();
    match derive_phase(conversation, is_subagent) {
        DshPhase::Bootstrap => {}
        DshPhase::CompactionRecovery => {
            keep.extend(COMPACTION_TOOLS.iter().map(|s| (*s).to_string()));
        }
        DshPhase::Promoted => {
            keep.extend(DISCOVERY_TOOLS.iter().map(|s| (*s).to_string()));
            keep.extend(unlocked_tool_names(conversation));
        }
    }
    keep
}

/// Filter advertised tool names for the current phase.
///
/// If a bootstrap tool is missing from `available` during [`DshPhase::Bootstrap`],
/// degrade to the full catalog (same as the plugin: composition drift must not
/// brick the session).
pub fn filter_tool_names(
    available: &[String],
    conversation: &[ConversationItem],
    is_subagent: bool,
) -> Vec<String> {
    if derive_phase(conversation, is_subagent) == DshPhase::Bootstrap {
        let missing = BOOTSTRAP_TOOLS
            .iter()
            .any(|required| !available.iter().any(|name| name == required));
        if missing {
            return available.to_vec();
        }
    }
    let keep = keep_tool_names(conversation, is_subagent);
    available
        .iter()
        .filter(|name| keep.contains(*name))
        .cloned()
        .collect()
}

/// Whether the conversation already carries the one-shot instruction hint.
pub fn conversation_has_instruction_hint(conversation: &[ConversationItem]) -> bool {
    conversation.iter().any(|item| {
        let ConversationItem::User(user) = item else {
            return false;
        };
        if user.synthetic_reason != Some(SyntheticReason::SystemReminder) {
            return false;
        }
        user.content.iter().any(|part| match part {
            xai_grok_sampling_types::ContentPart::Text { text } => {
                text.contains(INSTRUCTION_HINT_MARKER)
            }
            _ => false,
        })
    })
}

/// Build the post-promotion hint. Returns `None` when nothing was found.
pub fn format_instruction_hint(
    project_files: &[String],
    project_root: &str,
    user_global: bool,
) -> Option<String> {
    let mut sections = Vec::new();
    if !project_files.is_empty() {
        sections.push(format!(
            "Workspace instruction files exist: {} (project root: {}).",
            project_files.join(", "),
            project_root
        ));
    }
    if user_global {
        sections.push("A user-global instruction file exists: AGENTS.md.".to_string());
    }
    if sections.is_empty() {
        return None;
    }
    Some(format!(
        "{INSTRUCTION_HINT_MARKER} {} Do NOT assume their content. When a task touches this workspace, read the relevant instruction files first and follow them.",
        sections.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xai_grok_sampling_types::{AssistantItem, ContentPart, ToolCall, UserItem};

    fn assistant_with_tools(name: &str, arguments: &str) -> ConversationItem {
        ConversationItem::Assistant(AssistantItem {
            content: "".into(),
            tool_calls: vec![ToolCall {
                id: "tc1".into(),
                name: name.to_string(),
                arguments: arguments.into(),
            }],
            model_id: None,
            model_fingerprint: None,
            reasoning_effort: None,
        })
    }

    fn system_reminder(text: &str) -> ConversationItem {
        ConversationItem::User(UserItem {
            content: vec![ContentPart::Text { text: text.into() }],
            synthetic_reason: Some(SyntheticReason::SystemReminder),
            cwd_generation: None,
            prior_turn_interrupt: None,
            prompt_index: None,
        })
    }

    #[test]
    fn empty_conversation_is_bootstrap() {
        assert_eq!(derive_phase(&[], false), DshPhase::Bootstrap);
        assert_eq!(
            derive_phase(&[ConversationItem::system("You are helpful.")], false),
            DshPhase::Bootstrap
        );
    }

    #[test]
    fn first_assistant_promotes_either_signal() {
        let conversation = vec![
            ConversationItem::system(PERSONA),
            ConversationItem::user("fix the bug"),
            ConversationItem::assistant("We need to inspect the file."),
        ];
        assert_eq!(derive_phase(&conversation, false), DshPhase::Promoted);
    }

    #[test]
    fn first_tool_call_also_promotes() {
        let conversation = vec![
            ConversationItem::user("list files"),
            assistant_with_tools("bash", r#"{"command":"ls"}"#),
        ];
        assert_eq!(derive_phase(&conversation, false), DshPhase::Promoted);
    }

    #[test]
    fn subagent_is_always_promoted() {
        assert_eq!(derive_phase(&[], true), DshPhase::Promoted);
        let keep = keep_tool_names(&[], true);
        assert!(keep.contains("dev_tool_search"));
        assert!(keep.contains("bash"));
    }

    #[test]
    fn compaction_resets_until_new_assistant() {
        let conversation = vec![
            ConversationItem::user("old work"),
            ConversationItem::assistant("We need to edit src.rs"),
            ConversationItem::user_meta(
                "This session is being continued from a previous conversation",
            ),
        ];
        assert_eq!(
            derive_phase(&conversation, false),
            DshPhase::CompactionRecovery
        );
        let mut promoted = conversation.clone();
        promoted.push(ConversationItem::assistant("We need to continue."));
        assert_eq!(derive_phase(&promoted, false), DshPhase::Promoted);
    }

    #[test]
    fn last_compaction_is_the_boundary() {
        let conversation = vec![
            ConversationItem::user_meta("prefix"),
            ConversationItem::assistant("kept tail"),
            ConversationItem::user_meta("summary"),
        ];
        assert_eq!(
            derive_phase(&conversation, false),
            DshPhase::CompactionRecovery
        );
    }

    #[test]
    fn bootstrap_keep_set_is_minimal_pair() {
        let keep = keep_tool_names(&[], false);
        assert_eq!(
            keep,
            ["bash", "str_replace_editor"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
    }

    #[test]
    fn compaction_keep_set_adds_work_tools() {
        let conversation = vec![ConversationItem::user_meta("summary")];
        let keep = keep_tool_names(&conversation, false);
        assert!(keep.contains("bash"));
        assert!(keep.contains("read_file"));
        assert!(keep.contains("todo_write"));
        assert!(!keep.contains("dev_tool_search"));
        assert!(!keep.contains("web_search"));
    }

    #[test]
    fn promoted_keep_set_includes_unlocked_names() {
        let conversation = vec![
            ConversationItem::assistant("ok"),
            assistant_with_tools(
                "dev_tool_search",
                r#"{"query":"web","toolNames":["web_search","spawn_subagent"]}"#,
            ),
        ];
        let keep = keep_tool_names(&conversation, false);
        assert!(keep.contains("dev_tool_search"));
        assert!(keep.contains("skill_search"));
        assert!(keep.contains("skill_load"));
        assert!(keep.contains("web_search"));
        assert!(keep.contains("spawn_subagent"));
        assert!(!keep.contains("workflow"));
    }

    #[test]
    fn unlocked_names_ignore_malformed_arguments() {
        let conversation = vec![assistant_with_tools("dev_tool_search", "not-json")];
        assert!(unlocked_tool_names(&conversation).is_empty());
    }

    #[test]
    fn filter_degrades_to_full_catalog_when_bootstrap_pair_missing() {
        let available = vec!["read_file".into(), "grep".into()];
        assert_eq!(
            filter_tool_names(&available, &[], false),
            available,
            "missing bash/str_replace_editor must not brick bootstrap"
        );
    }

    #[test]
    fn filter_keeps_bootstrap_pair_when_present() {
        let available = vec![
            "bash".into(),
            "str_replace_editor".into(),
            "read_file".into(),
            "web_search".into(),
        ];
        assert_eq!(
            filter_tool_names(&available, &[], false),
            vec!["bash".to_string(), "str_replace_editor".to_string()]
        );
    }

    #[test]
    fn instruction_hint_format_and_dedup() {
        assert!(format_instruction_hint(&[], "/repo", false).is_none());
        let text =
            format_instruction_hint(&["AGENTS.md".into(), "CLAUDE.md".into()], "/repo", true)
                .unwrap();
        assert!(text.contains(INSTRUCTION_HINT_MARKER));
        assert!(text.contains("AGENTS.md, CLAUDE.md"));
        assert!(text.contains("user-global"));
        assert!(text.contains("Do NOT assume their content"));

        let conversation = vec![system_reminder(&text)];
        assert!(conversation_has_instruction_hint(&conversation));
        assert!(!conversation_has_instruction_hint(&[
            ConversationItem::user("hello")
        ]));
    }
}
