pub mod auto_update;
pub mod version;
mod version_policy;

pub use auto_update::UpdateStatus;
pub use version::{
    UpdateConfig, channel_label, channel_name, gh_release_repo, npm_package_name,
    version_for_update_check, write_version_cache,
};
pub use version_policy::enforce_version_policy_or_exit;
