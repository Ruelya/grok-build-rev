//! On-demand discovery tools for the anchored-standard resident catalog.
//!
//! Ports `dev-tool-search.mjs` and `skill-search.mjs`: the promoted phase
//! never dumps the full Standard catalog or `<available_skills>` block.

use crate::implementations::skills::skill::{build_skill_message, load_skill_content};
use crate::implementations::skills::types::SkillInfo;
use crate::types::output::ToolOutput;
use crate::types::requirements::{Expr, ToolRequirement};
use crate::types::resources::AvailableSkills;
use crate::types::tool::{ToolKind, ToolNamespace};
use crate::types::tool_io::ToolInput;

const MAX_TOOL_RESULTS: usize = 25;
const MAX_SKILL_RESULTS: usize = 20;

/// Full assembled catalog the model can search (resident + unlockable).
pub const SEARCH_CATALOG: &[(&str, &str)] = &[
    ("bash", "Run commands in a bash shell"),
    (
        "str_replace_editor",
        "View, create, and edit files (cat -n / unique str_replace / insert)",
    ),
    (
        "dev_tool_search",
        "Discover and unlock tools that are not currently available",
    ),
    ("skill_search", "Search available skills by keyword"),
    (
        "skill_load",
        "Load one skill's full instructions by exact name",
    ),
    ("web_search", "Search the internet"),
    ("web_fetch", "Fetch a URL and return its content"),
    ("spawn_subagent", "Delegate work to a sub-agent"),
    (
        "get_command_or_subagent_output",
        "Read output from a background command or subagent",
    ),
    (
        "wait_commands_or_subagents",
        "Wait for background commands or subagents",
    ),
    (
        "kill_command_or_subagent",
        "Kill a background command or subagent",
    ),
    ("workflow", "Run multi-agent workflow scripts"),
    ("update_goal", "Update a long-running goal"),
    ("todo_write", "Track tasks"),
    ("ask_user_question", "Ask the user a structured question"),
    ("read_file", "Read a file with line numbers"),
    ("search_replace", "Surgically replace text in a file"),
    ("write", "Create or overwrite a file"),
    ("list_dir", "List a directory"),
    ("grep", "Search file contents"),
    ("enter_plan_mode", "Enter structured plan mode"),
    ("exit_plan_mode", "Exit plan mode"),
    ("search_tool", "Search available MCP / plugin tools"),
    ("use_tool", "Invoke an MCP / plugin tool by name"),
    ("monitor", "Monitor background work"),
    ("scheduler_create", "Create a scheduled task"),
    ("scheduler_delete", "Delete a scheduled task"),
    ("scheduler_list", "List scheduled tasks"),
];

fn search_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Keyword search over [`SEARCH_CATALOG`]. Every token must match.
pub fn search_catalog(query: &str) -> Vec<(&'static str, &'static str)> {
    let wanted = search_tokens(query);
    SEARCH_CATALOG
        .iter()
        .copied()
        .filter(|(name, desc)| {
            if wanted.is_empty() {
                return true;
            }
            let haystack = format!("{name} {desc}").to_ascii_lowercase();
            wanted.iter().all(|token| haystack.contains(token))
        })
        .take(MAX_TOOL_RESULTS)
        .collect()
}

fn format_dev_tool_search(query: &str, unlock: &[String]) -> String {
    let mut lines = Vec::new();
    if !unlock.is_empty() {
        lines.push(format!(
            "Unlocked for the next request: {}",
            unlock.join(", ")
        ));
    }
    if query.is_empty() && unlock.is_empty() {
        lines.push(
            "Provide `query` to search the catalog, or `toolNames` to unlock tools.".to_string(),
        );
        return lines.join("\n");
    }
    if query.is_empty() {
        return lines.join("\n");
    }
    let matches = search_catalog(query);
    if matches.is_empty() {
        lines.push(format!("No tools match \"{query}\"."));
    } else {
        lines.push(format!("Matching tools ({}):", matches.len()));
        for (name, desc) in matches {
            let short: String = desc.chars().take(90).collect();
            lines.push(format!("- {name}: {short}"));
        }
        lines.push(r#"Unlock with dev_tool_search({"toolNames": ["<exact name>"]})."#.to_string());
    }
    lines.join("\n")
}

/// Input for `dev_tool_search`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DshDevToolSearchInput {
    /// Search keywords (e.g. "web", "subagent").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "search keywords (e.g. \"web\", \"subagent\")")]
    pub query: Option<String>,
    /// Exact tool names to unlock.
    #[serde(default, rename = "toolNames", skip_serializing_if = "Option::is_none")]
    #[schemars(description = "exact tool names to unlock")]
    pub tool_names: Option<Vec<String>>,
}

impl From<DshDevToolSearchInput> for ToolInput {
    fn from(value: DshDevToolSearchInput) -> Self {
        ToolInput::Dynamic(serde_json::to_value(value).expect("DshDevToolSearchInput serializes"))
    }
}

/// On-demand tool discovery and unlock.
#[derive(Debug, Default)]
pub struct DshDevToolSearchTool;

impl crate::types::tool_metadata::ToolMetadata for DshDevToolSearchTool {
    fn kind(&self) -> ToolKind {
        ToolKind::SearchTool
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        "Discover and unlock tools that are NOT currently available.\n\n\
This session starts with a minimal resident set: bash, str_replace_editor, skill_search, skill_load. Everything else is unlocked on demand through this tool.\n\n\
If the current task needs any of the following, call dev_tool_search FIRST — do not try to work around them with bash:\n\
- web_search / web_fetch — internet search and web retrieval\n\
- spawn_subagent — delegate work to sub-agents\n\
- workflow — run multi-agent workflow scripts\n\
- update_goal — long-running goals\n\
- todo_write — task tracking\n\
- ask_user_question — ask the user\n\
- read_file / write / search_replace / list_dir / grep — dedicated file tools\n\
- enter_plan_mode / exit_plan_mode — structured planning\n\
- search_tool / use_tool — MCP and plugin tools\n\
- get_command_or_subagent_output / wait_commands_or_subagents / kill_command_or_subagent — background jobs\n\n\
Usage: pass `query` to search the catalog (returns matching tool names + descriptions), then pass `toolNames` with exact names to unlock them. Unlocked tools appear from the next request on and stay unlocked for the session."
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl xai_tool_runtime::Tool for DshDevToolSearchTool {
    type Args = DshDevToolSearchInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("dev_tool_search").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "dev_tool_search",
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            is_read_only: true,
            tool_scope: Some(xai_tool_protocol::ToolScope::Read),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        _ctx: xai_tool_runtime::ToolCallContext,
        input: DshDevToolSearchInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let query = input.query.as_deref().unwrap_or("").trim();
        let unlock = input
            .tool_names
            .unwrap_or_default()
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>();
        Ok(ToolOutput::Text(
            format_dev_tool_search(query, &unlock).into(),
        ))
    }
}

/// Input for `skill_search`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DshSkillSearchInput {
    /// Search keywords (e.g. "pdf", "obsidian").
    #[schemars(description = "search keywords (e.g. \"pdf\", \"obsidian\", \"game review\")")]
    pub query: String,
}

impl From<DshSkillSearchInput> for ToolInput {
    fn from(value: DshSkillSearchInput) -> Self {
        ToolInput::Dynamic(serde_json::to_value(value).expect("DshSkillSearchInput serializes"))
    }
}

fn skill_invocable(skill: &SkillInfo) -> bool {
    skill.enabled && skill.user_invocable && !skill.disable_model_invocation
}

/// Filter skills whose name/description/when_to_use match every query token.
pub fn matching_skills<'a>(skills: &'a [SkillInfo], query: &str) -> Vec<&'a SkillInfo> {
    let wanted = search_tokens(query);
    skills
        .iter()
        .filter(|skill| skill_invocable(skill))
        .filter(|skill| {
            if wanted.is_empty() {
                return true;
            }
            let haystack = search_tokens(&format!(
                "{} {} {}",
                skill.name,
                skill.description,
                skill.when_to_use.as_deref().unwrap_or("")
            ))
            .join(" ");
            wanted.iter().all(|token| haystack.contains(token))
        })
        .collect()
}

/// On-demand skill discovery (no catalog injection).
#[derive(Debug, Default)]
pub struct DshSkillSearchTool;

impl crate::types::tool_metadata::ToolMetadata for DshSkillSearchTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Skill
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        "Search the available skills by keyword and return matching skill names with short descriptions. This session keeps NO skill catalog in the prompt — if a task looks like it matches a skill (document conversion, image processing, game reviews, markdown, PDF, spreadsheets, …), call skill_search FIRST to find it, then skill_load to activate it. Do NOT assume skill names from memory."
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl xai_tool_runtime::Tool for DshSkillSearchTool {
    type Args = DshSkillSearchInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("skill_search").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "skill_search",
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            is_read_only: true,
            tool_scope: Some(xai_tool_protocol::ToolScope::Read),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: DshSkillSearchInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let resources = crate::types::tool_metadata::shared_resources(&ctx)?;
        let skills = {
            let res = resources.lock().await;
            res.get::<AvailableSkills>()
                .map(|s| s.0.clone())
                .unwrap_or_default()
        };
        let matches = matching_skills(&skills, &input.query);
        if matches.is_empty() {
            return Ok(ToolOutput::Text(
                format!(
                    "No skills match \"{}\". Use skill_search with other keywords.",
                    input.query
                )
                .into(),
            ));
        }
        let extra = matches
            .len()
            .saturating_sub(MAX_SKILL_RESULTS)
            .checked_mul(1)
            .filter(|&n| n > 0)
            .map(|n| format!("\n…({n} more)"))
            .unwrap_or_default();
        let lines: Vec<String> = matches
            .iter()
            .take(MAX_SKILL_RESULTS)
            .map(|skill| {
                let desc = skill.description.lines().next().unwrap_or("");
                format!("- {}: {desc}", skill.name)
            })
            .collect();
        Ok(ToolOutput::Text(
            format!(
                "Matching skills ({}):\n{}\n\nLoad one with skill_load (exact name).{extra}",
                matches.len(),
                lines.join("\n")
            )
            .into(),
        ))
    }
}

/// Input for `skill_load`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DshSkillLoadInput {
    /// Exact skill name (kebab-case, from skill_search).
    #[schemars(description = "exact skill name (kebab-case, from skill_search)")]
    pub name: String,
}

impl From<DshSkillLoadInput> for ToolInput {
    fn from(value: DshSkillLoadInput) -> Self {
        ToolInput::Dynamic(serde_json::to_value(value).expect("DshSkillLoadInput serializes"))
    }
}

/// Load one skill body into the tool result (grok equivalent of `agent.inject`).
#[derive(Debug, Default)]
pub struct DshSkillLoadTool;

impl crate::types::tool_metadata::ToolMetadata for DshSkillLoadTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Skill
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        "Load the full instructions of ONE skill by its exact name (from skill_search results) and return them in this tool result. Call this before acting on a task that matches the skill."
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl xai_tool_runtime::Tool for DshSkillLoadTool {
    type Args = DshSkillLoadInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("skill_load").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "skill_load",
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            is_read_only: true,
            tool_scope: Some(xai_tool_protocol::ToolScope::Read),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: DshSkillLoadInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let resources = crate::types::tool_metadata::shared_resources(&ctx)?;
        let skills = {
            let res = resources.lock().await;
            res.get::<AvailableSkills>()
                .map(|s| s.0.clone())
                .unwrap_or_default()
        };
        let Some(skill) = skills
            .iter()
            .find(|s| s.name == input.name && skill_invocable(s))
        else {
            return Ok(ToolOutput::Text(
                format!(
                    "No skill named \"{}\". Run skill_search to list available skills.",
                    input.name
                )
                .into(),
            ));
        };
        match load_skill_content(skill).await {
            Ok(body) if body.is_empty() => Ok(ToolOutput::Text(
                format!("Skill \"{}\" has no loadable body.", input.name).into(),
            )),
            Ok(body) => Ok(ToolOutput::Text(build_skill_message(skill, &body).into())),
            Err(err) => Ok(ToolOutput::Text(format!("skill_load failed: {err}").into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_search_is_token_and() {
        let web = search_catalog("web");
        assert!(web.iter().any(|(n, _)| *n == "web_search"));
        assert!(web.iter().any(|(n, _)| *n == "web_fetch"));
        let none = search_catalog("definitely-not-a-tool-xyz");
        assert!(none.is_empty());
        let sub = search_catalog("subagent");
        assert!(sub.iter().any(|(n, _)| *n == "spawn_subagent"));
    }

    #[test]
    fn unlock_only_message() {
        let text = format_dev_tool_search("", &["web_search".into()]);
        assert_eq!(text, "Unlocked for the next request: web_search");
    }

    #[test]
    fn matching_skills_filters_disabled() {
        let mut enabled = SkillInfo::default();
        enabled.name = "pdf".into();
        enabled.description = "Convert PDF files".into();
        let mut disabled = enabled.clone();
        disabled.name = "secret".into();
        disabled.enabled = false;
        let skills = [enabled, disabled];
        let hits = matching_skills(&skills, "pdf");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "pdf");
    }

    #[test]
    fn serde_accepts_tool_names_camel_case() {
        let input: DshDevToolSearchInput =
            serde_json::from_str(r#"{"query":"web","toolNames":["web_search"]}"#).unwrap();
        assert_eq!(input.query.as_deref(), Some("web"));
        assert_eq!(
            input.tool_names.as_deref(),
            Some(&["web_search".to_string()][..])
        );
    }
}
