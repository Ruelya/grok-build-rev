//! Session-actor coverage for `dsh-anchored-standard` wiring.
//!
//! Phase keep-sets are unit-tested in `xai_grok_agent::dsh_anchored`. These
//! tests drive the shell chokepoints: advertised catalog, skill-catalog
//! suppression, one-shot instruction-hint, and prefix skip.

use super::support::{
    create_test_actor, test_dsh_anchored_agent, test_grok_build_agent_with_dsh_catalog,
};
use super::{PersistenceMsg, SessionActor};
use xai_grok_sampling_types::{ConversationItem, ToolCall};
use xai_grok_tools::types::skill_discovery_tracker::{SkillUpdateEffects, SkillUpdateKind};

fn tool_names(defs: &[crate::sampling::types::ToolDefinition]) -> Vec<String> {
    defs.iter().map(|d| d.function.name.clone()).collect()
}

async fn dsh_actor() -> SessionActor {
    let (gateway_tx, _grx) =
        tokio::sync::mpsc::unbounded_channel::<xai_acp_lib::AcpClientMessage>();
    let (persistence_tx, _prx) = tokio::sync::mpsc::unbounded_channel::<PersistenceMsg>();
    let actor = create_test_actor(0, 256_000, 85, gateway_tx, persistence_tx).await;
    *actor.agent.borrow_mut() = test_dsh_anchored_agent().await;
    actor
}

fn unlock_web_search() -> ConversationItem {
    ConversationItem::assistant_tool_calls(vec![ToolCall {
        id: "tc-unlock".into(),
        name: "dev_tool_search".into(),
        arguments: r#"{"query":"web","toolNames":["web_search"]}"#.into(),
    }])
}

#[tokio::test(flavor = "current_thread")]
async fn bootstrap_advertises_only_minimal_pair() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            actor
                .chat_state_handle
                .replace_conversation(vec![ConversationItem::user("fix the bug")]);
            let names = tool_names(&actor.prepare_tool_definitions_inner().await);
            assert_eq!(
                names,
                vec!["bash".to_string(), "str_replace_editor".to_string()],
                "request #1 must stay on the official Minimal pair"
            );
            assert!(actor.is_dsh_anchored_standard());
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn first_assistant_promotes_to_resident_catalog() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            actor.chat_state_handle.replace_conversation(vec![
                ConversationItem::user("fix the bug"),
                ConversationItem::assistant("We need to inspect the file."),
            ]);
            let names = tool_names(&actor.prepare_tool_definitions_inner().await);
            for required in [
                "bash",
                "str_replace_editor",
                "dev_tool_search",
                "skill_search",
                "skill_load",
            ] {
                assert!(
                    names.iter().any(|n| n == required),
                    "promoted catalog missing {required}: {names:?}"
                );
            }
            assert!(
                !names.iter().any(|n| n == "web_search"),
                "web_search must stay locked until unlock: {names:?}"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn compaction_recovery_exposes_work_set_not_discovery() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            actor.chat_state_handle.replace_conversation(vec![
                ConversationItem::user("old work"),
                ConversationItem::assistant("We need to edit src.rs"),
                ConversationItem::user_meta("session continued from a previous conversation"),
            ]);
            let names = tool_names(&actor.prepare_tool_definitions_inner().await);
            for required in [
                "bash",
                "str_replace_editor",
                "read_file",
                "write",
                "search_replace",
                "list_dir",
                "grep",
                "todo_write",
                "ask_user_question",
            ] {
                assert!(
                    names.iter().any(|n| n == required),
                    "compaction recovery missing {required}: {names:?}"
                );
            }
            assert!(
                !names.iter().any(|n| n == "dev_tool_search"),
                "discovery tools must wait for a new promotion signal: {names:?}"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn new_assistant_after_compaction_repromotes_and_keeps_unlocks() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            actor.chat_state_handle.replace_conversation(vec![
                ConversationItem::user_meta("summary"),
                ConversationItem::assistant("We need to continue."),
                unlock_web_search(),
            ]);
            let names = tool_names(&actor.prepare_tool_definitions_inner().await);
            assert!(names.iter().any(|n| n == "dev_tool_search"));
            assert!(
                names.iter().any(|n| n == "web_search"),
                "unlocked names must survive resume: {names:?}"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn grok_build_agent_is_not_phase_filtered() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (gateway_tx, _grx) =
                tokio::sync::mpsc::unbounded_channel::<xai_acp_lib::AcpClientMessage>();
            let (persistence_tx, _prx) = tokio::sync::mpsc::unbounded_channel::<PersistenceMsg>();
            let actor = create_test_actor(0, 256_000, 85, gateway_tx, persistence_tx).await;
            *actor.agent.borrow_mut() = test_grok_build_agent_with_dsh_catalog().await;
            actor
                .chat_state_handle
                .replace_conversation(vec![ConversationItem::user("hello")]);
            let names = tool_names(&actor.prepare_tool_definitions_inner().await);
            assert!(
                !actor.is_dsh_anchored_standard(),
                "control agent must not match the dsh name"
            );
            assert!(
                names.iter().any(|n| n == "web_search"),
                "grok-build must keep the full registered catalog: {names:?}"
            );
            assert!(names.iter().any(|n| n == "read_file"));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn wrap_skill_reminder_is_suppressed() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            let effects = SkillUpdateEffects {
                system_reminder: Some("<available_skills>pdf</available_skills>".into()),
                send_available_commands: true,
                kind: SkillUpdateKind::BaselineChange,
            };
            assert!(
                actor.wrap_skill_reminder(&effects).is_none(),
                "anchored-standard must never inject the skill catalog"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn instruction_hint_injects_once_after_promotion() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let workspace = tempfile::tempdir().expect("temp workspace");
            std::fs::create_dir(workspace.path().join(".git")).expect("git marker");
            std::fs::write(workspace.path().join("AGENTS.md"), "# project rules\n")
                .expect("AGENTS.md");

            let mut actor = dsh_actor().await;
            actor.session_info.cwd = workspace.path().display().to_string();
            actor.chat_state_handle.replace_conversation(vec![
                ConversationItem::user("fix the bug"),
                ConversationItem::assistant("We need to inspect the file."),
            ]);

            actor.maybe_inject_dsh_instruction_hint().await;
            let first = actor.chat_state_handle.get_conversation().await;
            let hints: Vec<_> = first
                .iter()
                .filter(|item| {
                    xai_grok_agent::dsh_anchored::conversation_has_instruction_hint(
                        std::slice::from_ref(*item),
                    )
                })
                .collect();
            assert_eq!(hints.len(), 1, "promotion must inject exactly one hint");
            let text = match hints[0] {
                ConversationItem::User(user) => user
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        xai_grok_sampling_types::ContentPart::Text { text } => Some(text.as_ref()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            assert!(text.contains("[dsh-instruction-hint]"));
            assert!(text.contains("AGENTS.md"));
            assert!(text.contains("Do NOT assume their content"));

            actor.maybe_inject_dsh_instruction_hint().await;
            let second = actor.chat_state_handle.get_conversation().await;
            let hint_count = second
                .iter()
                .filter(|item| {
                    xai_grok_agent::dsh_anchored::conversation_has_instruction_hint(
                        std::slice::from_ref(*item),
                    )
                })
                .count();
            assert_eq!(hint_count, 1, "hint must be one-shot");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn instruction_hint_skips_bootstrap() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let workspace = tempfile::tempdir().expect("temp workspace");
            std::fs::create_dir(workspace.path().join(".git")).expect("git marker");
            std::fs::write(workspace.path().join("AGENTS.md"), "# project rules\n")
                .expect("AGENTS.md");

            let mut actor = dsh_actor().await;
            actor.session_info.cwd = workspace.path().display().to_string();
            actor
                .chat_state_handle
                .replace_conversation(vec![ConversationItem::user("fix the bug")]);
            actor.maybe_inject_dsh_instruction_hint().await;
            let conv = actor.chat_state_handle.get_conversation().await;
            assert!(
                !xai_grok_agent::dsh_anchored::conversation_has_instruction_hint(&conv),
                "bootstrap must not receive the instruction hint"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn ensure_prefix_ready_aborts_without_injecting() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let actor = dsh_actor().await;
            actor
                .chat_state_handle
                .replace_conversation(vec![ConversationItem::user("hello")]);
            actor
                .deferred_prefix
                .arm(tokio::task::spawn_local(async { "PREFIX-LEAK".to_string() }));
            actor.ensure_prefix_ready().await;
            let conv = actor.chat_state_handle.get_conversation().await;
            let leaked = conv.iter().any(|item| match item {
                ConversationItem::User(user) => user.content.iter().any(|part| match part {
                    xai_grok_sampling_types::ContentPart::Text { text } => {
                        text.contains("PREFIX-LEAK")
                    }
                    _ => false,
                }),
                ConversationItem::System(sys) => sys.content.contains("PREFIX-LEAK"),
                _ => false,
            });
            assert!(!leaked, "dsh-anchored-standard must skip user prefix / AGENTS.md");
            assert!(
                actor.deferred_prefix.take().is_none(),
                "deferred prefix handle must be aborted and consumed"
            );
        })
        .await;
}
