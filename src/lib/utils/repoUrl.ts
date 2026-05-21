/**
 * 通用 Git 仓库 URL 解析与外链构造工具
 *
 * - 支持 GitHub: `https://github.com/owner/name(.git)?` 或 `owner/name`
 * - 支持 GitLab（含自建 / 嵌套 group）: `https://gitlab.corp.com/dept/team/project(.git)?`
 *
 * 纯字符串 `a/b/c` 由于无法判断 owner/name 边界，仅在两段时按 GitHub 处理；
 * GitLab 嵌套 group 必须提供完整 URL。
 */
export interface ParsedRepo {
  host: string;
  provider: "github" | "gitlab";
  owner: string; // GitLab 嵌套 group 时为 "a/b/c"
  name: string;
}

const KNOWN_GITHUB_HOSTS = new Set(["github.com", "www.github.com"]);

function inferProvider(host: string): "github" | "gitlab" {
  return KNOWN_GITHUB_HOSTS.has(host) ? "github" : "gitlab";
}

export function parseRepoUrl(input: string): ParsedRepo | null {
  const raw = input.trim();
  if (!raw) return null;

  // 形如 owner/name 的简写，仅 GitHub 适用
  if (!/^https?:\/\//i.test(raw) && !raw.includes("://")) {
    const cleaned = raw.replace(/^\/+|\/+$/g, "").replace(/\.git$/i, "");
    const parts = cleaned.split("/").filter(Boolean);
    if (parts.length === 2) {
      return {
        host: "github.com",
        provider: "github",
        owner: parts[0],
        name: parts[1],
      };
    }
    return null;
  }

  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    return null;
  }

  const host = u.hostname;
  if (!host) return null;

  const segs = u.pathname
    .replace(/\.git$/i, "")
    .split("/")
    .filter(Boolean);
  if (segs.length < 2) return null;

  const name = segs[segs.length - 1];
  const owner = segs.slice(0, -1).join("/");
  if (!owner || !name) return null;

  return {
    host,
    provider: inferProvider(host),
    owner,
    name,
  };
}

/** 构造仓库 Web 页面链接，自适应 GitHub / GitLab。 */
export function buildRepoWebUrl(repo: {
  host?: string;
  owner: string;
  name: string;
}): string {
  const host = repo.host || "github.com";
  return `https://${host}/${repo.owner}/${repo.name}`;
}

const FALLBACK_BRANCHES = new Set(["main", "master", "HEAD"]);

/** 判断 discoverable skill 是否来自指定仓库（host + owner + name，分支允许 main/master 互认）。 */
export function skillMatchesRepo(
  skill: {
    repoHost?: string;
    repoOwner: string;
    repoName: string;
    repoBranch?: string;
  },
  repo: {
    host?: string;
    owner: string;
    name: string;
    branch?: string;
  },
): boolean {
  const skillHost = (skill.repoHost || "github.com").toLowerCase();
  const repoHost = (repo.host || "github.com").toLowerCase();
  if (skillHost !== repoHost) return false;
  if (skill.repoOwner !== repo.owner || skill.repoName !== repo.name) return false;

  const repoBranch = repo.branch || "main";
  const skillBranch = skill.repoBranch || "main";
  if (skillBranch === repoBranch) return true;

  return FALLBACK_BRANCHES.has(skillBranch) && FALLBACK_BRANCHES.has(repoBranch);
}
