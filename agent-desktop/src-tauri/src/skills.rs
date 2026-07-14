// Skills 管理模块
// - 扫描本地 .codebuddy/skills/ 目录（内置/开发期技能）
// - 启用/禁用 skill（通过 .disabled 标记）
// - 市场来源：GitHub 搜索 + 本仓库托管的 skills（均可直连下载，无需外部 CLI）
// - 安装的技能存放于应用数据目录，由 Agent 的 LLM 在对话时注入 system prompt

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tauri::AppHandle;
use tauri::Manager;

/// 内置/开发期 Skills 目录（相对工作目录的 .codebuddy/skills/）
const SKILLS_DIR: &str = ".codebuddy/skills";
/// 用户从市场安装的 Skills 存放目录名（位于应用数据目录下，不混入源码树）
const INSTALLED_SKILLS_DIR: &str = "skills";

/// 技能仓库（用于远程 market.json 与内置市场兜底）
const SKILLS_REPO: &str = "346379/Agent";

/// 单个技能允许的最大体积（下载上限，防止异常大文件撑爆磁盘）
const MAX_SKILL_BYTES: u64 = 5 * 1024 * 1024;

/// 单个 Skill 的描述信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub description_zh: String,
    pub description_en: String,
    pub version: String,
    pub category: String,
    pub installed: bool,
    pub enabled: bool,
    /// 文件大小（bytes）
    pub size_bytes: u64,
}

/// Skills 市场的条目（统一接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMarketEntry {
    pub id: String,
    pub name: String,
    pub description_zh: String,
    pub description_en: String,
    pub version: String,
    pub category: String,
    /// 下载地址（GitHub raw URL 或空）
    #[serde(default)]
    pub download_url: String,
    pub size_bytes: u64,
    /// 是否已安装
    #[serde(default)]
    pub installed: bool,
    /// 数据来源: "clawhub" | "local"
    #[serde(default = "default_source")]
    pub source: String,
    /// ClawHub 下载量
    #[serde(default)]
    pub downloads: u64,
    /// ClawHub 星标数
    #[serde(default)]
    pub stars: u64,
}

fn default_source() -> String { "local".into() }

/// 获取 skills 根目录路径
fn skills_dir(_app: &AppHandle) -> PathBuf {
    // 优先用当前工作目录下的 .codebuddy/skills
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_path = cwd.join(SKILLS_DIR);
    if cwd_path.exists() {
        return cwd_path;
    }

    // 回退：相对于可执行文件的路径
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let exe_path = exe_dir.join(SKILLS_DIR);
    if exe_path.exists() {
        return exe_path;
    }

    // 最后回退到 cwd（即使不存在）
    cwd_path
}

/// 用户从市场安装的 Skills 目录（位于应用数据目录下，不混入源码树 .codebuddy）
fn installed_skills_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|p| p.join(INSTALLED_SKILLS_DIR))
        .unwrap_or_else(|_| skills_dir(app))
}

/// 所有需要扫描的 skills 目录：内置/开发期目录 + 用户安装目录
fn all_skill_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = vec![skills_dir(app), installed_skills_dir(app)];
    dirs.dedup();
    dirs
}

/// 扫描单个目录，返回其中的 Skill 列表
fn scan_skill_dir(dir: &PathBuf) -> Vec<SkillInfo> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    if let Some(info) = parse_skill_md(&skill_md) {
                        skills.push(info);
                    }
                }
            }
        }
    }
    skills
}

/// 解析 SKILL.md 的 YAML frontmatter，提取元信息
fn parse_skill_md(path: &PathBuf) -> Option<SkillInfo> {
    let content = fs::read_to_string(path).ok()?;
    let dir = path.parent()?;
    let id = dir.file_name()?.to_str()?.to_string();

    // 解析 frontmatter (--- ... ---)
    let body = content.strip_prefix("---")?;
    let (fm_str, _md_body) = body.split_once("---")?;

    let name = extract_yaml_str(fm_str, "name").unwrap_or(&id).to_string();
    let description = extract_yaml_str(fm_str, "description")
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let description_zh = extract_yaml_str(fm_str, "description_zh")
        .unwrap_or(&description)
        .to_string();
    let description_en = extract_yaml_str(fm_str, "description_en")
        .unwrap_or(&description)
        .to_string();
    let version = extract_yaml_str(fm_str, "version")
        .unwrap_or("0.1.0")
        .to_string();

    // 从 metadata.category 提取分类
    let category = extract_yaml_nested(fm_str, "metadata", "category")
        .unwrap_or("general")
        .to_string();

    // 检查是否被禁用（目录下有 .disabled 标记文件）
    let disabled_marker = dir.join(".disabled");
    let enabled = !disabled_marker.exists();

    // 计算目录大小
    let size_bytes = dir_size(dir);

    Some(SkillInfo {
        id,
        name,
        description,
        description_zh,
        description_en,
        version,
        category,
        installed: true,
        enabled,
        size_bytes,
    })
}

/// 从 YAML 字符串中提取顶级 key 的值（仅支持单行字符串）
fn extract_yaml_str<'a>(yaml: &'a str, key: &str) -> Option<&'a str> {
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{}:", key)) {
            let val = rest.trim();
            if val.is_empty() {
                continue; // 可能是嵌套对象
            }
            // 去掉首尾引号
            let val = val.trim_matches('"').trim_matches('\'');
            return Some(val);
        }
    }
    None
}

/// 从 YAML 中提取嵌套 key（如 metadata.category）
fn extract_yaml_nested<'a>(yaml: &'a str, parent: &str, key: &str) -> Option<&'a str> {
    let mut in_parent = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed == format!("{}:", parent) {
            in_parent = true;
            continue;
        }
        if in_parent {
            if let Some(rest) = trimmed.strip_prefix(&format!("{}:", key)) {
                return Some(rest.trim().trim_matches('"').trim_matches('\''));
            }
            // 如果不是缩进的，说明离开了 parent
            if !trimmed.starts_with(' ') && !trimmed.is_empty() {
                break;
            }
        }
    }
    None
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                total += p.metadata().map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

// ===================== 对话注入 =====================

/// 获取所有已启用 Skill 的合并 system prompt（供 chat_stream 注入）
/// 返回的是纯文本，可直接作为 system message 的 content
pub fn get_active_system_prompt(app: &AppHandle) -> String {
    let mut prompts: Vec<String> = Vec::new();
    for dir in all_skill_dirs(app) {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                // 跳过被禁用的
                if path.join(".disabled").exists() {
                    continue;
                }
                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                let content = match fs::read_to_string(&skill_md) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                // 去掉 YAML frontmatter，只保留正文内容
                let body = strip_frontmatter(&content);
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                if !body.trim().is_empty() {
                    prompts.push(format!(
                        "## Skill: {name}\n{body}"
                    ));
                }
            }
        }
    }

    if prompts.is_empty() {
        return String::new();
    }

    format!(
        "以下是启用的技能（Skills），请在对话中遵循这些技能的指导：\n\n{}",
        prompts.join("\n\n---\n\n")
    )
}

/// 去掉 SKILL.md 的 YAML frontmatter（--- ... ---）
fn strip_frontmatter(content: &str) -> String {
    let content = content.trim();
    if let Some(rest) = content.strip_prefix("---") {
        if let Some((_fm, body)) = rest.split_once("---") {
            return body.trim().to_string();
        }
    }
    // 没有 frontmatter，直接返回全部内容
    content.to_string()
}

// ===================== Tauri 命令 =====================

/// 列出本地已安装的所有 Skills（内置目录 + 用户安装目录，按 id 去重）
#[tauri::command]
pub fn skills_list(app: AppHandle) -> Result<Vec<SkillInfo>, String> {
    let mut by_id: std::collections::HashMap<String, SkillInfo> = std::collections::HashMap::new();
    for dir in all_skill_dirs(&app) {
        for skill in scan_skill_dir(&dir) {
            // 用户安装目录优先（后写覆盖先写）
            by_id.insert(skill.id.clone(), skill);
        }
    }

    let mut skills: Vec<SkillInfo> = by_id.into_values().collect();

    // 按分类排序
    skills.sort_by(|a, b| a.category.cmp(&b.category).then(a.id.cmp(&b.id)));
    Ok(skills)
}

/// 启用或禁用一个 Skill（在内置或安装目录中查找）
#[tauri::command]
pub fn skills_toggle(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let dir = find_skill_dir(&app, &id)
        .ok_or_else(|| format!("Skill '{}' 未找到", id))?;

    let marker = dir.join(".disabled");
    if enabled {
        if marker.exists() {
            fs::remove_file(&marker).map_err(|e| format!("启用失败: {}", e))?;
        }
    } else {
        fs::write(&marker, "disabled").map_err(|e| format!("禁用失败: {}", e))?;
    }

    Ok(())
}

/// 在全部 skills 目录中查找指定 id 的目录
fn find_skill_dir(app: &AppHandle, id: &str) -> Option<PathBuf> {
    for dir in all_skill_dirs(app) {
        let candidate = dir.join(id);
        if candidate.join("SKILL.md").exists() {
            return Some(candidate);
        }
    }
    None
}

/// 市场数据源：每个 provider 返回一组市场条目（单个源失败不影响其它源）
type MarketFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<SkillMarketEntry>, String>> + Send>>;
type MarketProvider = fn() -> MarketFuture;

/// 获取 Skills 市场列表（多数据源合并：GitHub 搜索 → 远程 market.json → 内置兜底）
#[tauri::command]
pub async fn skills_market_list() -> Result<Vec<SkillMarketEntry>, String> {
    let providers: Vec<MarketProvider> = vec![market_github, market_local];

    let mut merged: Vec<SkillMarketEntry> = Vec::new();
    let mut has_online = false;

    for provider in providers {
        match provider().await {
            Ok(items) => {
                if !items.is_empty() {
                    has_online = true;
                }
                // 同 id 不重复（先到先得，在线源优先于兜底）
                for item in items {
                    if !merged.iter().any(|e| e.id == item.id) {
                        merged.push(item);
                    }
                }
            }
            Err(e) => eprintln!("[skills] 市场数据源失败: {e}"),
        }
    }

    // 全部在线源失败时才用内置兜底列表（保证离线可用）
    if !has_online {
        for item in builtin_market() {
            if !merged.iter().any(|e| e.id == item.id) {
                merged.push(item);
            }
        }
    }

    // 按下载量降序排列
    merged.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    Ok(merged)
}

/// 数据源 1：GitHub 主题搜索
fn market_github() -> MarketFuture {
    Box::pin(async move { fetch_github_skills().await })
}

/// 数据源 2：远程 market.json（仓库内维护的索引）
fn market_local() -> MarketFuture {
    Box::pin(async move { fetch_local_market().await })
}



/// 根据 slug 和 topics 推断分类（保留备用）
#[allow(dead_code)]
fn infer_category(slug: &str, topics: &[String]) -> String {
    let slug_lower = slug.to_lowercase();
    let topics_str = topics.join(" ").to_lowercase();

    if slug_lower.contains("web") || slug_lower.contains("frontend") || topics_str.contains("web") || topics_str.contains("react") || topics_str.contains("vue") {
        "frontend".into()
    } else if slug_lower.contains("cloud") || slug_lower.contains("server") || topics_str.contains("cloud") || topics_str.contains("backend") {
        "backend".into()
    } else if slug_lower.contains("mini") || slug_lower.contains("wechat") || topics_str.contains("miniprogram") {
        "frontend".into()
    } else if slug_lower.contains("ai") || slug_lower.contains("agent") || slug_lower.contains("ml") || topics_str.contains("ai") || topics_str.contains("agent") {
        "ai".into()
    } else if slug_lower.contains("search") || slug_lower.contains("mcp") || topics_str.contains("mcp") {
        "mcp".into()
    } else if slug_lower.contains("research") || slug_lower.contains("marketing") || slug_lower.contains("playbook") || topics_str.contains("research") {
        "research".into()
    } else if slug_lower.contains("design") || slug_lower.contains("ui") || topics_str.contains("design") {
        "tools".into()
    } else {
        "general".into()
    }
}

/// 从 GitHub 搜索 Skills（topic:codebuddy-skill + topic:claude-skill）
async fn fetch_github_skills() -> Result<Vec<SkillMarketEntry>, String> {
    let urls = [
        "https://api.github.com/search/repositories?q=topic:codebuddy-skill&sort=stars&per_page=20",
        "https://api.github.com/search/repositories?q=topic:claude-skill&sort=stars&per_page=20",
    ];

    let mut entries: Vec<SkillMarketEntry> = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"));
            h
        })
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if resp.status() == 403 {
            eprintln!("[skills] GitHub API 限流");
            continue;
        }
        if !resp.status().is_success() {
            continue;
        }
        #[derive(Debug, Deserialize)]
        struct GitHubSearchResponse {
            items: Vec<GitHubSkillRepo>,
        }
        #[derive(Debug, Deserialize)]
        struct GitHubSkillRepo {
            full_name: String,
            description: Option<String>,
            stargazers_count: u64,
            #[allow(dead_code)]
            html_url: String,
            #[allow(dead_code)]
            topics: Option<Vec<String>>,
        }

        match resp.json::<GitHubSearchResponse>().await {
            Ok(data) => {
                for repo in data.items {
                    let id = repo.full_name.replace('/', "-");
                    let name = repo.full_name.clone();
                    let desc = repo.description.clone().unwrap_or_default();
                    let raw_base = format!(
                        "https://raw.githubusercontent.com/{}/main",
                        repo.full_name
                    );
                    entries.push(SkillMarketEntry {
                        id,
                        name,
                        description_zh: String::new(),
                        description_en: desc,
                        version: "0.1.0".into(),
                        category: "general".into(),
                        download_url: raw_base,
                        size_bytes: 0,
                        installed: false,
                        source: "github".into(),
                        downloads: 0,
                        stars: repo.stargazers_count,
                
                    });
                }
            }
            Err(e) => eprintln!("[skills] GitHub 解析失败: {}", e),
        }
    }

    Ok(entries)
}

/// 从远程 GitHub 获取本地 market.json
async fn fetch_local_market() -> Result<Vec<SkillMarketEntry>, String> {
    let market_url =
        format!("https://raw.githubusercontent.com/{SKILLS_REPO}/main/.codebuddy/skills/market.json");
    let resp = reqwest::get(market_url)
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Vec<SkillMarketEntry>>()
        .await
        .map_err(|e| format!("JSON 解析失败: {e}"))
}

/// 通过 ClawHub CLI 安装技能（保留备用，当前市场不再使用 ClawHub 源）
#[allow(dead_code)]
#[tauri::command]
pub async fn skills_clawhub_install(id: String) -> Result<String, String> {
    // 尝试运行 clawhub CLI 安装
    let mut std_cmd = std::process::Command::new("clawhub");
    std_cmd.creation_flags(0x08000000);
    std_cmd.args(["skill", "install", &id]);
    let output = tokio::process::Command::from(std_cmd)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            Ok(stdout)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            // 如果 CLI 未安装，返回提示
            if stderr.contains("not found") || stderr.contains("No such file") {
                Err(format!(
                    "clawhub CLI 未安装。请运行: npm i -g clawhub && clawhub login && clawhub skill install {id}"
                ))
            } else {
                Err(format!("安装失败: {}", stderr))
            }
        }
        Err(_) => Err(format!(
            "clawhub CLI 未安装。请运行: npm i -g clawhub && clawhub login && clawhub skill install {id}"
        )),
    }
}

/// 从 GitHub(raw URL) 下载并安装一个 Skill（递归下载整个目录，支持 main/master 分支回退）
#[tauri::command]
pub async fn skills_install(app: AppHandle, id: String, download_url: String) -> Result<(), String> {
    if download_url.is_empty() {
        return Err("该技能缺少下载地址，无法安装".into());
    }

    let (owner, repo, branch, subpath) = parse_raw_github_url(&download_url)
        .ok_or_else(|| format!("无法解析下载地址: {download_url}"))?;

    let dir = installed_skills_dir(&app).join(&id);

    // 如果已存在，先备份再覆盖
    if dir.exists() {
        let backup = installed_skills_dir(&app).join(format!("{}.bak", id));
        let _ = fs::remove_dir_all(&backup);
        fs::rename(&dir, &backup).map_err(|e| format!("备份失败: {}", e))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let client = github_client();

    // 主分支尝试；404 时回退 master
    let result = install_from_github(&client, &owner, &repo, &branch, &subpath, &dir).await;
    let result = match result {
        Err(e) if e == "NOT_FOUND" && branch == "main" => {
            install_from_github(&client, &owner, &repo, "master", &subpath, &dir)
                .await
                .map_err(|e2| format!("{e}（已尝试 master 分支: {e2}）"))
        }
        other => other,
    };

    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }

    // 校验：必须是有效 Skill（含 SKILL.md）
    if !dir.join("SKILL.md").exists() {
        let _ = fs::remove_dir_all(&dir);
        return Err("下载的内容不是有效 Skill（缺少 SKILL.md）".into());
    }

    Ok(())
}

/// 解析 raw.githubusercontent.com 目录 URL -> (owner, repo, branch, subpath)
fn parse_raw_github_url(url: &str) -> Option<(String, String, String, String)> {
    let path = url
        .strip_prefix("https://raw.githubusercontent.com/")
        .or_else(|| url.strip_prefix("http://raw.githubusercontent.com/"))?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    let branch = parts[2].to_string();
    let subpath = if parts.len() > 3 {
        parts[3..].join("/")
    } else {
        String::new()
    };
    Some((owner, repo, branch, subpath))
}

/// 构造带超时与 UA 的 GitHub HTTP 客户端
fn github_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("votek/0.1")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 从 GitHub 下载整个 skill 目录到 dest；404 时返回 Err("NOT_FOUND")
async fn install_from_github(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    subpath: &str,
    dest: &PathBuf,
) -> Result<(), String> {
    let api_url = if subpath.is_empty() {
        format!("https://api.github.com/repos/{owner}/{repo}/contents?ref={branch}")
    } else {
        format!(
            "https://api.github.com/repos/{owner}/{repo}/contents/{subpath}?ref={branch}"
        )
    };
    fetch_github_contents(client, &api_url, subpath, dest).await?;
    Ok(())
}

/// 递归列举并下载 GitHub 目录内容（支持子目录；同 id 文件按相对路径落盘）
async fn fetch_github_contents(
    client: &reqwest::Client,
    api_url: &str,
    base_subpath: &str,
    dest: &PathBuf,
) -> Result<u64, String> {
    let resp = client
        .get(api_url)
        .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("列出目录失败: {e}"))?;

    if resp.status() == 404 {
        return Err("NOT_FOUND".into());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub 目录列表失败 (HTTP {})", resp.status()));
    }

    #[derive(serde::Deserialize)]
    struct GHEntry {
        #[serde(rename = "type")]
        ty: String,
        path: String,
        #[serde(default)]
        download_url: Option<String>,
        #[serde(default)]
        url: Option<String>,
    }

    let entries: Vec<GHEntry> = resp
        .json()
        .await
        .map_err(|e| format!("解析目录失败: {e}"))?;

    let mut total: u64 = 0;
    for entry in entries {
        // 相对目标路径：相对于当前目录的 base_subpath 计算
        let rel = if base_subpath.is_empty() {
            entry.path.clone()
        } else {
            entry
                .path
                .strip_prefix(&format!("{base_subpath}/"))
                .unwrap_or(&entry.path)
                .to_string()
        };

        if entry.ty == "dir" {
            let dir_dest = dest.join(&rel);
            let _ = fs::create_dir_all(&dir_dest);
            // 递归时以当前目录的完整路径作为新的 base
            if let Some(url) = entry.url {
                total += Box::pin(fetch_github_contents(
                    client,
                    &url,
                    &entry.path,
                    &dir_dest,
                ))
                .await?;
            }
        } else if entry.ty == "file" {
            if let Some(dl) = entry.download_url {
                let bytes = client
                    .get(&dl)
                    .send()
                    .await
                    .map_err(|e| format!("下载 {} 失败: {}", entry.path, e))?
                    .bytes()
                    .await
                    .map_err(|e| format!("读取 {} 失败: {}", entry.path, e))?;
                total += bytes.len() as u64;
                if total > MAX_SKILL_BYTES {
                    return Err("技能体积超过 5MB 上限".into());
                }
                let out = dest.join(&rel);
                if let Some(p) = out.parent() {
                    let _ = fs::create_dir_all(p);
                }
                fs::write(&out, &bytes)
                    .map_err(|e| format!("写入 {} 失败: {}", entry.path, e))?;
            }
        }
    }
    Ok(total)
}

/// 删除本地 Skill（仅允许删除用户安装的技能）
#[tauri::command]
pub fn skills_delete(app: AppHandle, id: String) -> Result<(), String> {
    let dir = installed_skills_dir(&app).join(&id);
    if !dir.exists() {
        return Err(format!("Skill '{}' 未找到（仅用户安装的技能可删除）", id));
    }

    fs::remove_dir_all(&dir).map_err(|e| format!("删除失败: {}", e))?;
    Ok(())
}

/// 预览当前所有已启用 Skill 会注入的 system prompt（调试用）
#[tauri::command]
pub fn skills_preview_prompt(app: AppHandle) -> Result<String, String> {
    Ok(get_active_system_prompt(&app))
}

/// 打开 SKILL.md 文件（返回内容供前端预览）
#[tauri::command]
pub fn skills_read_content(app: AppHandle, id: String) -> Result<String, String> {
    let dir = find_skill_dir(&app, &id)
        .ok_or_else(|| format!("Skill '{}' 的 SKILL.md 不存在", id))?;
    let path = dir.join("SKILL.md");
    fs::read_to_string(&path).map_err(|e| format!("读取失败: {}", e))
}

// ===================== 内置市场列表 =====================

fn builtin_market() -> Vec<SkillMarketEntry> {
    let base = format!("https://raw.githubusercontent.com/{SKILLS_REPO}/main/.codebuddy/skills");
    let local = || SkillMarketEntry {
        source: "local".into(),
        downloads: 0,
        stars: 0,

        // Placeholder fields set below
        id: String::new(),
        name: String::new(),
        description_zh: String::new(),
        description_en: String::new(),
        version: String::new(),
        category: String::new(),
        download_url: String::new(),
        size_bytes: 0,
        installed: false,
    };
    vec![
        SkillMarketEntry { id: "frontend-dev".into(), name: "前端开发工作室".into(), description_zh: "全栈前端开发：精美 UI 设计、电影级动画、AI 媒体生成、转化文案。构建落地页、营销网站、产品页、仪表盘。".into(), description_en: "Full-stack frontend: premium UI, cinematic animations, AI media, persuasive copy.".into(), version: "0.1.1".into(), category: "frontend".into(), download_url: format!("{base}/frontend-dev"), size_bytes: 14480, ..local() },
        SkillMarketEntry { id: "fullstack-dev".into(), name: "全栈开发".into(), description_zh: "后端架构 + 前后端集成：Express/Next.js/FastAPI/Go，REST API，实时功能(SSE/WebSocket)，认证，文件上传。".into(), description_en: "Backend architecture & fullstack integration: REST APIs, real-time, auth, file uploads.".into(), version: "0.1.0".into(), category: "backend".into(), download_url: format!("{base}/fullstack-dev"), size_bytes: 13970, ..local() },
        SkillMarketEntry { id: "mcp-builder".into(), name: "MCP 服务器构建器".into(), description_zh: "构建高质量 MCP 服务器：Python/Node SDK，工具设计，stdio 传输，让 LLM 调用外部服务。".into(), description_en: "Build MCP servers: Python/Node SDK, tool design, stdio transport, LLM ↔ external services.".into(), version: "0.1.0".into(), category: "mcp".into(), download_url: format!("{base}/mcp-builder"), size_bytes: 7500, ..local() },
        SkillMarketEntry { id: "prompt-engineering-expert".into(), name: "Prompt 工程专家".into(), description_zh: "高级 Prompt 工程：自定义指令设计、提示词优化、AI Agent 行为调优。".into(), description_en: "Advanced prompt engineering: custom instructions, prompt optimization, AI agent tuning.".into(), version: "0.1.0".into(), category: "ai".into(), download_url: format!("{base}/prompt-engineering-expert"), size_bytes: 1630, ..local() },
        SkillMarketEntry { id: "agent-team-orchestration".into(), name: "智能体团队编排".into(), description_zh: "多智能体团队编排：角色定义、任务流转、交接协议、质量门禁。".into(), description_en: "Multi-agent team orchestration: roles, task routing, handoff protocols, quality gates.".into(), version: "0.1.0".into(), category: "ai".into(), download_url: format!("{base}/agent-team-orchestration"), size_bytes: 5100, ..local() },
        SkillMarketEntry { id: "deep-research".into(), name: "深度研究".into(), description_zh: "结构化深度研究：生成大纲、并行搜索、Markdown 报告。支持学术研究、技术选型、市场分析。".into(), description_en: "Structured deep research: outline generation, parallel search, Markdown reports.".into(), version: "0.1.0".into(), category: "research".into(), download_url: format!("{base}/deep-research"), size_bytes: 2325, ..local() },
        SkillMarketEntry { id: "browser-use".into(), name: "浏览器自动化".into(), description_zh: "浏览器交互自动化：网页测试、表单填写、截图、数据提取。".into(), description_en: "Browser automation: web testing, form filling, screenshots, data extraction.".into(), version: "0.1.0".into(), category: "tools".into(), download_url: format!("{base}/browser-use"), size_bytes: 7760, ..local() },
    ]
}
