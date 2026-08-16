//! DeepSeek Harness Anchored Standard tools.
//!
//! Model-facing names and schemas match the official Minimal pair
//! (`bash` + `str_replace_editor`) plus the plugin's discovery surface
//! (`dev_tool_search`, `skill_search`, `skill_load`).

pub mod bash;
pub mod discovery;
pub mod str_replace_editor;

pub use bash::{DshBashInput, DshBashTool};
pub use discovery::{
    DshDevToolSearchInput, DshDevToolSearchTool, DshSkillLoadInput, DshSkillLoadTool,
    DshSkillSearchInput, DshSkillSearchTool,
};
pub use str_replace_editor::{DshStrReplaceEditorInput, DshStrReplaceEditorTool};
