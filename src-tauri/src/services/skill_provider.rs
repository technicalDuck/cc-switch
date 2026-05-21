//! Git host provider abstraction for Skills
//!
//! 把 Skills 仓库的归档下载 / 文档链接 / 分支解析 / 鉴权头封装到一个 trait 中，
//! 这样新增 GitLab 等内网仓库时只需要实现新的 provider，而不必散弹式修改 SkillService。
//!
//! 当前实现：
//! - [`GithubProvider`]：复刻原有 GitHub 行为（github.com 归档 zip）
//! - [`GitlabProvider`]：支持自托管 GitLab，含嵌套 group 的 owner、PRIVATE-TOKEN 鉴权

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::services::skill::SkillRepo;

/// 抽象 Git 主机能力
pub trait GitHostProvider: Send + Sync {
    /// 归档 ZIP 下载 URL（按指定分支）
    fn archive_zip_url(&self, repo: &SkillRepo, branch: &str) -> String;

    /// 备选归档 ZIP URL（按优先级排在 [`Self::archive_zip_url`] 之前尝试）
    fn archive_zip_fallback_urls(&self, _repo: &SkillRepo, _branch: &str) -> Vec<String> {
        Vec::new()
    }

    /// 仓库内某文档的 web 链接（用于 readme_url）
    fn blob_url(&self, repo: &SkillRepo, branch: &str, path: &str) -> String;

    /// 外链：仓库主页（用于「打开仓库」按钮）
    fn repo_web_url(&self, repo: &SkillRepo) -> String;

    /// 从 source_url 中解析分支名（如 GitHub 的 `/tree/<branch>` 或 GitLab 的 `/-/tree/<branch>`）
    fn parse_branch_from_url(&self, source_url: &str) -> Option<String>;

    /// 鉴权请求头（如 GitLab 的 `PRIVATE-TOKEN`）
    fn auth_headers(&self, token: Option<&str>) -> HeaderMap;
}

// ========== 工具函数 ==========

/// 仅对 `/` 做 percent-encode，用于把 "dept/team/proj" 编为 "dept%2Fteam%2Fproj"
/// （GitLab 的归档 API 要求把整个 namespace+project 当作单个路径段）
pub(crate) fn percent_encode_slashes(input: &str) -> String {
    input.replace('/', "%2F")
}

// ========== GitHub Provider ==========

pub struct GithubProvider;

impl GitHostProvider for GithubProvider {
    fn archive_zip_url(&self, repo: &SkillRepo, branch: &str) -> String {
        // 与改造前完全一致
        format!(
            "https://{}/{}/{}/archive/refs/heads/{}.zip",
            repo.host, repo.owner, repo.name, branch
        )
    }

    fn blob_url(&self, repo: &SkillRepo, branch: &str, path: &str) -> String {
        format!(
            "https://{}/{}/{}/blob/{}/{}",
            repo.host, repo.owner, repo.name, branch, path
        )
    }

    fn repo_web_url(&self, repo: &SkillRepo) -> String {
        format!("https://{}/{}/{}", repo.host, repo.owner, repo.name)
    }

    fn parse_branch_from_url(&self, source_url: &str) -> Option<String> {
        let trimmed = source_url.trim();
        if trimmed.is_empty() {
            return None;
        }
        // /tree/<branch>/...
        if let Some((_, after_tree)) = trimmed.split_once("/tree/") {
            let branch = after_tree
                .split('/')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            return Some(branch.to_string());
        }
        None
    }

    fn auth_headers(&self, _token: Option<&str>) -> HeaderMap {
        // GitHub 当前匿名访问，无需鉴权头
        HeaderMap::new()
    }
}

// ========== GitLab Provider ==========

pub struct GitlabProvider;

impl GitHostProvider for GitlabProvider {
    fn archive_zip_url(&self, repo: &SkillRepo, branch: &str) -> String {
        // GitLab 归档 URL 形态：
        //   https://{host}/{namespace}/{project}/-/archive/{branch}/{project}-{branch}.zip
        // 当 namespace 含嵌套 group 时（owner = "dept/team"），需要把 owner/name 整体作为
        // 单个项目路径段，因此对其中的 '/' 做 percent-encode。
        let path = percent_encode_slashes(&format!("{}/{}", repo.owner, repo.name));
        let safe_branch = encode_branch(branch);
        let file_branch = repo.name.trim();
        format!(
            "https://{}/{}/-/archive/{}/{}-{}.zip",
            repo.host, path, safe_branch, file_branch, safe_branch
        )
    }

    fn archive_zip_fallback_urls(&self, repo: &SkillRepo, branch: &str) -> Vec<String> {
        // 私有 GitLab 的 Web 归档端点常返回登录页 HTML；API v4 + PRIVATE-TOKEN 更可靠。
        let path = percent_encode_slashes(&format!("{}/{}", repo.owner, repo.name));
        let safe_branch = encode_branch(branch);
        vec![format!(
            "https://{}/api/v4/projects/{}/repository/archive.zip?sha={}",
            repo.host, path, safe_branch
        )]
    }

    fn blob_url(&self, repo: &SkillRepo, branch: &str, path: &str) -> String {
        // GitLab 用 `/-/blob/<branch>/<path>`；owner/name 是普通路径（不做编码）
        format!(
            "https://{}/{}/{}/-/blob/{}/{}",
            repo.host, repo.owner, repo.name, branch, path
        )
    }

    fn repo_web_url(&self, repo: &SkillRepo) -> String {
        format!("https://{}/{}/{}", repo.host, repo.owner, repo.name)
    }

    fn parse_branch_from_url(&self, source_url: &str) -> Option<String> {
        let trimmed = source_url.trim();
        if trimmed.is_empty() {
            return None;
        }
        // /-/tree/<branch>/...
        if let Some((_, after_tree)) = trimmed.split_once("/-/tree/") {
            let branch = after_tree
                .split('/')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            return Some(branch.to_string());
        }
        None
    }

    fn auth_headers(&self, token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let Some(token) = token.map(|t| t.trim()).filter(|t| !t.is_empty()) else {
            return headers;
        };
        match HeaderValue::from_str(token) {
            Ok(value) => {
                headers.insert(HeaderName::from_static("private-token"), value);
            }
            Err(_) => {
                log::warn!("GitLab Token 含非法字符，跳过 PRIVATE-TOKEN 头注入");
            }
        }
        headers
    }
}

/// 分支名可能含 `/`（如 `feat/foo`），归档 URL 内需要做最小转义
fn encode_branch(branch: &str) -> String {
    branch.replace('/', "%2F")
}

// ========== 入口 ==========

/// 根据 SkillRepo 的 provider 字段拿到对应的 provider 实例
pub fn resolve_provider(repo: &SkillRepo) -> Box<dyn GitHostProvider> {
    resolve_provider_by_id(&repo.provider)
}

/// 按 provider 标识拿 provider；未识别的标识回退到 GitHub，保持向后兼容
pub fn resolve_provider_by_id(provider: &str) -> Box<dyn GitHostProvider> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "gitlab" => Box::new(GitlabProvider),
        _ => Box::new(GithubProvider),
    }
}

/// URL → (host, provider, owner, name) 的探测
///
/// - GitHub: 仅 host == "github.com" 时识别为 github
/// - 其他 host 一律视为 GitLab（含自托管 gitlab.corp.com）
/// - owner 可以包含嵌套路径（GitLab group）
///
/// 输入可以是：
/// - 完整 URL: `https://gitlab.corp.com/dept/team/proj`
/// - 带 .git 后缀: `https://github.com/owner/repo.git`
/// - 简短形式: `owner/repo`（按 github.com 处理）
pub fn detect_from_url(input: &str) -> Option<(String, String, String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 简短形式：单独的 owner/name（GitHub 兼容）
    if !trimmed.contains("://") && !trimmed.starts_with("//") {
        let cleaned = trimmed.trim_end_matches(".git").trim_matches('/');
        let parts: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() == 2 {
            return Some((
                "github.com".to_string(),
                "github".to_string(),
                parts[0].to_string(),
                parts[1].to_string(),
            ));
        }
        return None;
    }

    let parsed = url::Url::parse(trimmed).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();

    let raw_path = parsed.path().trim_matches('/');
    let cleaned_path = raw_path.trim_end_matches(".git");
    let segments: Vec<&str> = cleaned_path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }

    let name = segments.last().unwrap().to_string();
    let owner = segments[..segments.len() - 1].join("/");
    let provider = if host == "github.com" {
        "github".to_string()
    } else {
        "gitlab".to_string()
    };

    Some((host, provider, owner, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo(host: &str, provider: &str, owner: &str, name: &str, branch: &str) -> SkillRepo {
        SkillRepo {
            host: host.to_string(),
            provider: provider.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            branch: branch.to_string(),
            enabled: true,
        }
    }

    #[test]
    fn github_archive_url_matches_legacy_format() {
        let p = GithubProvider;
        let repo = make_repo("github.com", "github", "anthropics", "skills", "main");
        assert_eq!(
            p.archive_zip_url(&repo, "main"),
            "https://github.com/anthropics/skills/archive/refs/heads/main.zip"
        );
    }

    #[test]
    fn github_blob_url_uses_blob_path() {
        let p = GithubProvider;
        let repo = make_repo("github.com", "github", "anthropics", "skills", "main");
        assert_eq!(
            p.blob_url(&repo, "main", "foo/SKILL.md"),
            "https://github.com/anthropics/skills/blob/main/foo/SKILL.md"
        );
    }

    #[test]
    fn github_parse_branch_from_tree_url() {
        let p = GithubProvider;
        assert_eq!(
            p.parse_branch_from_url("https://github.com/owner/repo/tree/dev/skills"),
            Some("dev".to_string())
        );
        assert_eq!(
            p.parse_branch_from_url("https://github.com/owner/repo"),
            None
        );
    }

    #[test]
    fn gitlab_api_archive_url_uses_v4_endpoint() {
        let p = GitlabProvider;
        let repo = make_repo("gitlab.corp.com", "gitlab", "tianmafront", "team-skills", "master");
        let urls = p.archive_zip_fallback_urls(&repo, "master");
        assert_eq!(urls.len(), 1);
        assert_eq!(
            urls[0],
            "https://gitlab.corp.com/api/v4/projects/tianmafront%2Fteam-skills/repository/archive.zip?sha=master"
        );
    }

    #[test]
    fn gitlab_archive_url_encodes_nested_namespace() {
        let p = GitlabProvider;
        let repo = make_repo("gitlab.corp.com", "gitlab", "dept/team", "proj", "main");
        assert_eq!(
            p.archive_zip_url(&repo, "main"),
            "https://gitlab.corp.com/dept%2Fteam%2Fproj/-/archive/main/proj-main.zip"
        );
    }

    #[test]
    fn gitlab_blob_url_uses_dash_blob_path() {
        let p = GitlabProvider;
        let repo = make_repo("gitlab.corp.com", "gitlab", "dept", "proj", "main");
        assert_eq!(
            p.blob_url(&repo, "main", "skills/foo/SKILL.md"),
            "https://gitlab.corp.com/dept/proj/-/blob/main/skills/foo/SKILL.md"
        );
    }

    #[test]
    fn gitlab_parse_branch_from_dash_tree_url() {
        let p = GitlabProvider;
        assert_eq!(
            p.parse_branch_from_url("https://gitlab.corp.com/dept/proj/-/tree/main/skills"),
            Some("main".to_string())
        );
    }

    #[test]
    fn gitlab_auth_headers_inject_private_token_when_provided() {
        let p = GitlabProvider;
        let headers = p.auth_headers(Some("glpat-xxx"));
        assert_eq!(
            headers.get("private-token").and_then(|v| v.to_str().ok()),
            Some("glpat-xxx")
        );
    }

    #[test]
    fn gitlab_auth_headers_empty_when_no_token() {
        let p = GitlabProvider;
        assert!(p.auth_headers(None).is_empty());
        assert!(p.auth_headers(Some("   ")).is_empty());
    }

    #[test]
    fn detect_from_url_github_short_form() {
        let (host, provider, owner, name) =
            detect_from_url("anthropics/skills").expect("parses owner/name");
        assert_eq!(host, "github.com");
        assert_eq!(provider, "github");
        assert_eq!(owner, "anthropics");
        assert_eq!(name, "skills");
    }

    #[test]
    fn detect_from_url_github_full_url() {
        let (host, provider, owner, name) =
            detect_from_url("https://github.com/anthropics/skills.git").expect("parses github url");
        assert_eq!(host, "github.com");
        assert_eq!(provider, "github");
        assert_eq!(owner, "anthropics");
        assert_eq!(name, "skills");
    }

    #[test]
    fn detect_from_url_gitlab_nested_group() {
        let (host, provider, owner, name) =
            detect_from_url("https://gitlab.corp.com/a/b/c/proj").expect("parses gitlab url");
        assert_eq!(host, "gitlab.corp.com");
        assert_eq!(provider, "gitlab");
        assert_eq!(owner, "a/b/c");
        assert_eq!(name, "proj");
    }

    #[test]
    fn detect_from_url_rejects_single_segment() {
        assert!(detect_from_url("https://gitlab.corp.com/only").is_none());
        assert!(detect_from_url("solo").is_none());
    }
}
