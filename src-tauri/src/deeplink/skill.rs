//! Skill import from deep link
//!
//! Handles importing skill repository configurations via ccswitch:// URLs.

use super::DeepLinkImportRequest;
use crate::error::AppError;
use crate::services::skill::SkillRepo;
use crate::services::skill_provider::detect_from_url;
use crate::store::AppState;

/// Import a skill from deep link request
///
/// `repo` 字段同时接受两种形式：
/// - 旧 GitHub 兼容：`owner/name`
/// - 新通用形式：完整 URL，如 `https://gitlab.corp.com/dept/team/proj`
pub fn import_skill_from_deeplink(
    state: &AppState,
    request: DeepLinkImportRequest,
) -> Result<String, AppError> {
    // Verify this is a skill request
    if request.resource != "skill" {
        return Err(AppError::InvalidInput(format!(
            "Expected skill resource, got '{}'",
            request.resource
        )));
    }

    // Parse repo
    let repo_str = request
        .repo
        .ok_or_else(|| AppError::InvalidInput("Missing 'repo' field for skill".to_string()))?;

    let (host, provider, owner, name) = detect_from_url(repo_str.trim()).ok_or_else(|| {
        AppError::InvalidInput(format!(
            "Invalid repo format: expected 'owner/name' or full Git URL, got '{repo_str}'"
        ))
    })?;

    let repo = SkillRepo {
        host: host.clone(),
        provider,
        owner: owner.clone(),
        name: name.clone(),
        branch: request.branch.unwrap_or_else(|| "main".to_string()),
        enabled: request.enabled.unwrap_or(true),
    };

    // Save using Database
    state.db.save_skill_repo(&repo)?;

    log::info!("Successfully added skill repo '{host}/{owner}/{name}'");

    Ok(format!("{owner}/{name}"))
}
