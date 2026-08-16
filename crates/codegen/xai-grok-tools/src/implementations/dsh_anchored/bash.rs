//! Official DeepSeek Harness Minimal `bash` — one required parameter: `command`.
//!
//! Description and parameter schema are taken from the official Minimal
//! composition (`apps/cli/config/agent-presets/minimal/agent.cordis.yml` +
//! `@deepseek-ai/dsh-tool-bash-persistent`). Execution delegates to
//! [`crate::implementations::grok_build::BashTool`].

use crate::implementations::grok_build::bash::{BashTool, BashToolInput, BashToolOutput};
use crate::types::requirements::{Expr, ToolRequirement};
use crate::types::tool::{ToolKind, ToolNamespace};
use crate::types::tool_io::ToolInput;

/// Official Minimal first-request bash description (persistent-shell row).
const DESCRIPTION: &str = "\
Run commands in a bash shell
* When invoking this tool, the contents of the \"command\" parameter does NOT need to be XML-escaped.
* You don't have access to the internet via this tool.
* You do have access to a mirror of common linux and python packages via apt and pip.
* State is persistent across command calls and discussions with the user.
* To inspect a particular line range of a file, e.g. lines 10-25, try 'sed -n 10,25p /path/to/the/file'.
* Please avoid commands that may produce a very large amount of output.
* Please run long lived commands in the background, e.g. 'sleep 10 &' or start a server in the background.";

/// Input for the official Minimal `bash` tool — `command` only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DshBashInput {
    /// The bash command to run. Relative path is preferred in the command.
    #[schemars(
        description = "The bash command to run. Relative path is preferred in the command."
    )]
    pub command: String,
}

impl From<DshBashInput> for ToolInput {
    fn from(value: DshBashInput) -> Self {
        ToolInput::Dynamic(serde_json::to_value(value).expect("DshBashInput serializes"))
    }
}

/// Minimal-schema bash tool (client name `bash`).
#[derive(Debug, Default)]
pub struct DshBashTool;

impl crate::types::tool_metadata::ToolMetadata for DshBashTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Execute
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        DESCRIPTION
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl xai_tool_runtime::Tool for DshBashTool {
    type Args = DshBashInput;
    type Output = BashToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("bash").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "bash",
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            is_read_only: false,
            tool_scope: Some(xai_tool_protocol::ToolScope::Write),
            ..Default::default()
        }
    }

    #[tracing::instrument(name = "tool.dsh_bash", skip_all)]
    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: DshBashInput,
    ) -> Result<BashToolOutput, xai_tool_runtime::ToolError> {
        if input.command.trim().is_empty() {
            return Err(xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new("bash").expect("valid"),
                "command must be a non-empty string",
            ));
        }
        let mapped = BashToolInput {
            command: input.command,
            timeout: None,
            description: "dsh bash".to_string(),
            is_background: false,
        };
        BashTool.run(ctx, mapped).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;

    #[test]
    fn schema_exposes_only_command() {
        let schema = schema_for!(DshBashInput);
        let value = serde_json::to_value(schema).unwrap();
        let required = value["required"].as_array().unwrap();
        assert_eq!(required, &vec![serde_json::json!("command")]);
        let props = value["properties"].as_object().unwrap();
        assert_eq!(props.len(), 1);
        assert!(props.contains_key("command"));
        assert!(
            !props.contains_key("timeout"),
            "Minimal bash must not advertise timeout"
        );
        assert!(
            !props.contains_key("description"),
            "Minimal bash must not advertise description"
        );
        assert!(
            !props.contains_key("is_background"),
            "Minimal bash must not advertise is_background"
        );
    }

    #[test]
    fn id_is_bash() {
        assert_eq!(xai_tool_runtime::Tool::id(&DshBashTool).as_str(), "bash");
    }

    #[test]
    fn description_matches_official_minimal_yaml() {
        assert_eq!(
            DESCRIPTION,
            "Run commands in a bash shell\n\
* When invoking this tool, the contents of the \"command\" parameter does NOT need to be XML-escaped.\n\
* You don't have access to the internet via this tool.\n\
* You do have access to a mirror of common linux and python packages via apt and pip.\n\
* State is persistent across command calls and discussions with the user.\n\
* To inspect a particular line range of a file, e.g. lines 10-25, try 'sed -n 10,25p /path/to/the/file'.\n\
* Please avoid commands that may produce a very large amount of output.\n\
* Please run long lived commands in the background, e.g. 'sleep 10 &' or start a server in the background."
        );
    }
}
