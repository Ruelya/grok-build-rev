//! Official DeepSeek Harness `str_replace_editor`.
//!
//! Schema and model-facing strings match
//! `@deepseek-ai/dsh-tool-str-replace-editor` (`view` / `create` /
//! `str_replace` / `insert`). File I/O goes through [`FileSystem`];
//! directory `view` uses `tokio::fs` because the grok filesystem seam
//! has no `list_dir`.

use std::path::Path;

use crate::notification::types::FileWritten;
use crate::types::output::ToolOutput;
use crate::types::requirements::{Expr, ToolRequirement};
use crate::types::resources::{DisplayCwd, FileSystem, NotificationHandle, resolve_model_path};
use crate::types::tool::{ToolKind, ToolNamespace};
use crate::types::tool_io::ToolInput;

const MAX_OUTPUT_CHARS: usize = 16_000;
const TRUNCATED_MESSAGE: &str = "<response clipped><NOTE>To save on context only part of this file has been shown to you. You should retry this tool after you have searched inside the file with `grep -n` in order to find the line numbers of what you are looking for.</NOTE>";

/// Official `DEFAULT_DESCRIPTION` from `dsh-tool-str-replace-editor`.
const DESCRIPTION: &str = "\
Custom editing tool for viewing, creating and editing files
* State is persistent across command calls and discussions with the user
* If `path` is a file, `view` displays the result of applying `cat -n`. If `path` is a directory, `view` lists non-hidden files and directories up to 2 levels deep
* The `create` command cannot be used if the specified `path` already exists as a file
* If a `command` generates a long output, it will be truncated and marked with `<response clipped>`

Notes for using the `str_replace` command:
* The `old_str` parameter should match EXACTLY one or more consecutive lines from the original file. Be mindful of whitespaces!
* If the `old_str` parameter is not unique in the file, the replacement will not be performed. Make sure to include enough context in `old_str` to make it unique
* The `new_str` parameter should contain the edited lines that should replace the `old_str`";

/// Commands accepted by the official Minimal editor.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DshEditorCommand {
    View,
    Create,
    StrReplace,
    Insert,
}

/// Official Minimal `str_replace_editor` parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DshStrReplaceEditorInput {
    /// The commands to run. Allowed options are: `view`, `create`, `str_replace`, `insert`.
    #[schemars(
        description = "The commands to run. Allowed options are: `view`, `create`, `str_replace`, `insert`."
    )]
    pub command: DshEditorCommand,
    /// Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.
    #[schemars(
        description = "Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`."
    )]
    pub path: String,
    /// Required parameter of `create` command, with the content of the file to be created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Required parameter of `create` command, with the content of the file to be created."
    )]
    pub file_text: Option<String>,
    /// Required parameter of `insert` command. The `new_str` will be inserted AFTER the line `insert_line` of `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Required parameter of `insert` command. The `new_str` will be inserted AFTER the line `insert_line` of `path`."
    )]
    pub insert_line: Option<i64>,
    /// Optional parameter of `str_replace` command containing the new string (if not given, no string will be added). Required parameter of `insert` command containing the string to insert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional parameter of `str_replace` command containing the new string (if not given, no string will be added). Required parameter of `insert` command containing the string to insert."
    )]
    pub new_str: Option<String>,
    /// Required parameter of `str_replace` command containing the string in `path` to replace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Required parameter of `str_replace` command containing the string in `path` to replace."
    )]
    pub old_str: Option<String>,
    /// Optional parameter of `view` command when `path` points to a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional parameter of `view` command when `path` points to a file. If none is given, the full file is shown. If provided, the file will be shown in the indicated line number range, e.g. [11, 12] will show lines 11 and 12. Indexing at 1 to start. Setting `[start_line, -1]` shows all lines from `start_line` to the end of the file."
    )]
    pub view_range: Option<Vec<i64>>,
}

impl From<DshStrReplaceEditorInput> for ToolInput {
    fn from(value: DshStrReplaceEditorInput) -> Self {
        ToolInput::Dynamic(
            serde_json::to_value(value).expect("DshStrReplaceEditorInput serializes"),
        )
    }
}

/// Official Minimal `str_replace_editor` tool.
#[derive(Debug, Default)]
pub struct DshStrReplaceEditorTool;

impl crate::types::tool_metadata::ToolMetadata for DshStrReplaceEditorTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Edit
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        DESCRIPTION
    }

    fn emitted_notifications(&self) -> &'static [&'static str] {
        &["FileWritten"]
    }

    fn requires_expr(&self) -> Expr<ToolRequirement> {
        Expr::True
    }
}

impl xai_tool_runtime::Tool for DshStrReplaceEditorTool {
    type Args = DshStrReplaceEditorInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("str_replace_editor").expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "str_replace_editor",
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

    #[tracing::instrument(name = "tool.str_replace_editor", skip_all)]
    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: DshStrReplaceEditorInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let text = run_editor(ctx, input).await?;
        Ok(ToolOutput::Text(text.into()))
    }
}

fn tool_err(message: impl Into<String>) -> xai_tool_runtime::ToolError {
    xai_tool_runtime::ToolError::execution(
        xai_tool_protocol::ToolId::new("str_replace_editor").expect("valid"),
        message.into(),
    )
}

fn require_absolute(path: &str) -> Result<(), xai_tool_runtime::ToolError> {
    if path.trim().is_empty() {
        return Err(tool_err("path must be a non-empty string"));
    }
    if !Path::new(path).is_absolute() {
        return Err(tool_err(format!(
            "The path {path} is not an absolute path, it should start with `/`. Maybe you meant /{path}?"
        )));
    }
    Ok(())
}

fn maybe_truncate(content: String) -> String {
    if content.len() <= MAX_OUTPUT_CHARS {
        content
    } else {
        format!("{}{TRUNCATED_MESSAGE}", &content[..MAX_OUTPUT_CHARS])
    }
}

/// `cat -n` style view used by official Minimal.
pub fn format_file_view(
    path: &str,
    content: &str,
    view_range: Option<&[i64]>,
) -> Result<String, String> {
    let all_lines: Vec<&str> = content.split('\n').collect();
    let total = all_lines.len();
    let mut initial_line: i64 = 1;
    let mut lines: &[&str] = &all_lines;
    let mut prompt = format!(
        "Here's the content of {path} with line numbers (which has a total of {total} lines)"
    );
    if let Some(range) = view_range {
        if range.len() != 2 {
            return Err("Invalid `view_range`. It should be a list of two integers.".into());
        }
        let requested_initial = range[0];
        let requested_final = range[1];
        if requested_initial < 1 || requested_initial > total as i64 {
            return Err(format!(
                "Invalid `view_range`: [{requested_initial}, {requested_final}]. Its first element `{requested_initial}` should be within the range of lines of the file: [1, {total}]"
            ));
        }
        if requested_final > total as i64 {
            return Err(format!(
                "Invalid `view_range`: [{requested_initial}, {requested_final}]. Its second element `{requested_final}` should be smaller than the number of lines in the file: `{total}`"
            ));
        }
        if requested_final != -1 && requested_final < requested_initial {
            return Err(format!(
                "Invalid `view_range`: [{requested_initial}, {requested_final}]. Its second element `{requested_final}` should be larger or equal than its first `{requested_initial}`"
            ));
        }
        initial_line = requested_initial;
        let start = (requested_initial as usize).saturating_sub(1);
        lines = if requested_final == -1 {
            &all_lines[start..]
        } else {
            &all_lines[start..requested_final as usize]
        };
        prompt.push_str(&format!(
            " with view_range=[{requested_initial}, {requested_final}]"
        ));
    }
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{:>6}  {line}", initial_line + index as i64))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(maybe_truncate(format!("{prompt}:\n{numbered}\n")))
}

fn match_offsets(content: &str, search: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut from = 0;
    while let Some(found) = content[from..].find(search) {
        let offset = from + found;
        offsets.push(offset);
        from = offset + search.len();
    }
    offsets
}

fn line_numbers_at(content: &str, offsets: &[usize]) -> Vec<usize> {
    let mut line = 1usize;
    let mut cursor = 0usize;
    offsets
        .iter()
        .map(|&offset| {
            while cursor < offset {
                if content.as_bytes().get(cursor) == Some(&b'\n') {
                    line += 1;
                }
                cursor += 1;
            }
            line
        })
        .collect()
}

/// Unique `str_replace`. `new_str` defaults to empty (official behavior).
pub fn apply_str_replace(
    content: &str,
    old_str: &str,
    new_str: &str,
    path: &str,
) -> Result<String, String> {
    if old_str.is_empty() {
        return Err("Parameter `old_str` is empty for command: str_replace".into());
    }
    let offsets = match_offsets(content, old_str);
    match offsets.as_slice() {
        [] => Err(format!(
            "No replacement was performed, old_str `{old_str}` did not appear verbatim in {path}."
        )),
        [_] => {
            let offset = offsets[0];
            Ok(format!(
                "{}{new_str}{}",
                &content[..offset],
                &content[offset + old_str.len()..]
            ))
        }
        _ => {
            let lines = line_numbers_at(content, &offsets);
            let joined = lines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "No replacement was performed. Multiple occurrences of old_str `{old_str}` in lines [{joined}]. Please ensure it is unique"
            ))
        }
    }
}

/// Insert `new_str` after `insert_line` (0 = beginning of file).
pub fn apply_insert(content: &str, insert_line: i64, new_str: &str) -> Result<String, String> {
    let lines: Vec<&str> = content.split('\n').collect();
    if insert_line < 0 || insert_line > lines.len() as i64 {
        return Err(format!(
            "Invalid `insert_line` parameter: {insert_line}. It should be within the range of lines of the file: [0, {}]",
            lines.len()
        ));
    }
    let at = insert_line as usize;
    let mut out = Vec::with_capacity(lines.len() + new_str.split('\n').count());
    out.extend_from_slice(&lines[..at]);
    out.extend(new_str.split('\n'));
    out.extend_from_slice(&lines[at..]);
    Ok(out.join("\n"))
}

fn skip_dir_entry(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "__pycache__"
}

async fn list_directory(path: &Path, display: &str) -> Result<String, xai_tool_runtime::ToolError> {
    async fn visit(dir: &Path, depth: usize) -> Result<Vec<String>, std::io::Error> {
        let mut rows = Vec::new();
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if skip_dir_entry(&name) {
                continue;
            }
            let child = entry.path();
            let is_dir = entry.file_type().await?.is_dir();
            let kind = if is_dir { "d" } else { "f" };
            rows.push(format!("{kind}\t{}", child.display()));
            if is_dir && depth < 2 {
                rows.extend(Box::pin(visit(&child, depth + 1)).await?);
            }
        }
        Ok(rows)
    }
    let mut rows = vec![format!("d\t{display}")];
    match visit(path, 1).await {
        Ok(children) => rows.extend(children),
        Err(err) => return Err(tool_err(err.to_string())),
    }
    rows.sort_by(|left, right| {
        let left_path = left.split_once('\t').map(|(_, p)| p).unwrap_or(left);
        let right_path = right.split_once('\t').map(|(_, p)| p).unwrap_or(right);
        left_path.cmp(right_path)
    });
    let listing = maybe_truncate(format!("{}\n", rows.join("\n")));
    Ok(format!(
        "Here're the files and directories up to 2 levels deep in {display}, excluding hidden items, node_modules, and Python cache directories:\n{listing}\n"
    ))
}

async fn run_editor(
    ctx: xai_tool_runtime::ToolCallContext,
    input: DshStrReplaceEditorInput,
) -> Result<String, xai_tool_runtime::ToolError> {
    require_absolute(&input.path)?;
    let resources = crate::types::tool_metadata::shared_resources(&ctx)?;
    let (cwd, display_cwd, fs, notification_handle) = {
        let cwd = crate::types::tool_metadata::resolve_cwd(&ctx, &resources).await?;
        let res = resources.lock().await;
        let display_cwd = res.get::<DisplayCwd>().map(|d| d.0.clone());
        let fs = res.require::<FileSystem>()?.0.clone();
        let notification_handle = res.require::<NotificationHandle>()?.0.clone();
        (cwd, display_cwd, fs, notification_handle)
    };
    let resolved = resolve_model_path(&cwd, display_cwd.as_deref(), &input.path);
    let display = input.path.clone();
    let tool_call_id = ctx.call_id.as_str().to_owned();

    match input.command {
        DshEditorCommand::View => {
            view_path(
                fs.as_ref(),
                &resolved,
                &display,
                input.view_range.as_deref(),
            )
            .await
        }
        DshEditorCommand::Create => {
            let file_text = input
                .file_text
                .ok_or_else(|| tool_err("Parameter `file_text` is required for command: create"))?;
            create_file(
                fs.as_ref(),
                &notification_handle,
                &resolved,
                &display,
                &file_text,
                &tool_call_id,
            )
            .await
        }
        DshEditorCommand::StrReplace => {
            let old_str = input.old_str.ok_or_else(|| {
                tool_err("Parameter `old_str` is required for command: str_replace")
            })?;
            let new_str = input.new_str.unwrap_or_default();
            mutate_file(
                fs.as_ref(),
                &notification_handle,
                &resolved,
                &display,
                &tool_call_id,
                |before| apply_str_replace(before, &old_str, &new_str, &display),
            )
            .await
        }
        DshEditorCommand::Insert => {
            let insert_line = input.insert_line.ok_or_else(|| {
                tool_err("Parameter `insert_line` is required for command: insert")
            })?;
            let new_str = input
                .new_str
                .ok_or_else(|| tool_err("Parameter `new_str` is required for command: insert"))?;
            mutate_file(
                fs.as_ref(),
                &notification_handle,
                &resolved,
                &display,
                &tool_call_id,
                |before| apply_insert(before, insert_line, &new_str),
            )
            .await
        }
    }
}

async fn view_path(
    fs: &dyn crate::computer::types::AsyncFileSystem,
    path: &Path,
    display: &str,
    view_range: Option<&[i64]>,
) -> Result<String, xai_tool_runtime::ToolError> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| {
        tool_err(format!(
            "The path {display} does not exist. Please provide a valid path."
        ))
    })?;
    if meta.is_dir() {
        if view_range.is_some() {
            return Err(tool_err(
                "The `view_range` parameter is not allowed when `path` points to a directory.",
            ));
        }
        return list_directory(path, display).await;
    }
    if !meta.is_file() {
        return Err(tool_err(format!(
            "cannot view \"{display}\": not a regular file or directory"
        )));
    }
    let bytes = fs
        .read_file(path)
        .await
        .map_err(|e| tool_err(e.to_string()))?;
    let content = String::from_utf8_lossy(&bytes);
    format_file_view(display, &content, view_range).map_err(tool_err)
}

async fn create_file(
    fs: &dyn crate::computer::types::AsyncFileSystem,
    notification_handle: &crate::notification::types::ToolNotificationHandle,
    path: &Path,
    display: &str,
    file_text: &str,
    tool_call_id: &str,
) -> Result<String, xai_tool_runtime::ToolError> {
    if tokio::fs::metadata(path).await.is_ok() {
        return Err(tool_err(format!(
            "File already exists at: {display}. Cannot overwrite files using command `create`."
        )));
    }
    fs.write_file(path, file_text.as_bytes())
        .await
        .map_err(|e| tool_err(e.to_string()))?;
    notification_handle.send_file_written(FileWritten {
        tool_call_id: tool_call_id.to_string(),
        absolute_path: path.to_path_buf(),
        content: file_text.to_string(),
        previous_content: None,
        is_new_file: true,
    });
    Ok(format!("New file created successfully at: {display}"))
}

async fn mutate_file(
    fs: &dyn crate::computer::types::AsyncFileSystem,
    notification_handle: &crate::notification::types::ToolNotificationHandle,
    path: &Path,
    display: &str,
    tool_call_id: &str,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<String, xai_tool_runtime::ToolError> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| {
        tool_err(format!(
            "The path {display} does not exist. Please provide a valid path."
        ))
    })?;
    if meta.is_dir() {
        return Err(tool_err(format!(
            "The path {display} is a directory and only the `view` command can be used on directories"
        )));
    }
    let bytes = fs
        .read_file(path)
        .await
        .map_err(|e| tool_err(e.to_string()))?;
    let before = String::from_utf8_lossy(&bytes).into_owned();
    let after = edit(&before).map_err(tool_err)?;
    fs.write_file(path, after.as_bytes())
        .await
        .map_err(|e| tool_err(e.to_string()))?;
    notification_handle.send_file_written(FileWritten {
        tool_call_id: tool_call_id.to_string(),
        absolute_path: path.to_path_buf(),
        content: after,
        previous_content: Some(before),
        is_new_file: false,
    });
    Ok(format!("The file {display} has been edited successfully."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_numbers_lines_like_cat_n() {
        let out = format_file_view("/repo/a.rs", "alpha\nbeta\n", None).unwrap();
        assert!(out.contains("     1  alpha"));
        assert!(out.contains("     2  beta"));
        assert!(out.contains("total of 3 lines"));
    }

    #[test]
    fn view_range_and_negative_end() {
        let content = "a\nb\nc\nd";
        let ranged = format_file_view("/f", content, Some(&[2, 3])).unwrap();
        assert!(ranged.contains("     2  b"));
        assert!(ranged.contains("     3  c"));
        assert!(!ranged.contains("     1  a"));
        let rest = format_file_view("/f", content, Some(&[3, -1])).unwrap();
        assert!(rest.contains("     3  c"));
        assert!(rest.contains("     4  d"));
    }

    #[test]
    fn view_range_rejects_bad_bounds() {
        assert!(format_file_view("/f", "a\nb", Some(&[0, 1])).is_err());
        assert!(format_file_view("/f", "a\nb", Some(&[1, 9])).is_err());
        assert!(format_file_view("/f", "a\nb", Some(&[2, 1])).is_err());
        assert!(format_file_view("/f", "a\nb", Some(&[1])).is_err());
    }

    #[test]
    fn str_replace_requires_unique_match() {
        let ok = apply_str_replace("hello world", "world", "there", "/f").unwrap();
        assert_eq!(ok, "hello there");
        let empty_new = apply_str_replace("hello world", " world", "", "/f").unwrap();
        assert_eq!(empty_new, "hello");
        let missing = apply_str_replace("hello", "nope", "x", "/f").unwrap_err();
        assert!(missing.contains("did not appear verbatim"));
        let dup = apply_str_replace("aa\naa", "aa", "b", "/f").unwrap_err();
        assert!(dup.contains("Multiple occurrences"));
        assert!(dup.contains("lines [1, 2]"));
    }

    #[test]
    fn insert_after_line_including_zero() {
        let at_start = apply_insert("b\nc", 0, "a").unwrap();
        assert_eq!(at_start, "a\nb\nc");
        let after_first = apply_insert("a\nc", 1, "b").unwrap();
        assert_eq!(after_first, "a\nb\nc");
        assert!(apply_insert("a", 3, "x").is_err());
    }

    #[test]
    fn command_serializes_as_official_snake_case() {
        let value = serde_json::to_value(DshEditorCommand::StrReplace).unwrap();
        assert_eq!(value, serde_json::json!("str_replace"));
    }

    #[test]
    fn id_is_str_replace_editor() {
        assert_eq!(
            xai_tool_runtime::Tool::id(&DshStrReplaceEditorTool).as_str(),
            "str_replace_editor"
        );
    }
}
