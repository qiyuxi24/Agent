// Skills 管理模块
// - 扫描本地 .codebuddy/skills/ 目录
// - 启用/禁用 skill（通过 .disabled 标记）
// - ClawHub 市场 API 集成（主数据源）
// - GitHub raw 下载作为安装方式

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

/// Skills 根目录（相对于项目根目录的 .codebuddy/skills/）
const SKILLS_DIR: &str = ".codebuddy/skills";

/// ClawHub API 地址
const CLAWHUB_API: &str = "https://clawhub.ai/api/v1/skills";

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
    /// 是否需要通过 clawhub CLI 安装
    #[serde(default)]
    pub external_install: bool,
}

fn default_source() -> String { "local".into() }

/// ClawHub API 返回的技能条目
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClawHubSkill {
    slug: String,
    #[serde(rename = "displayName")]
    display_name: String,
    summary: String,
    description: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
    tags: Option<serde_json::Value>,
    stats: Option<ClawHubStats>,
    #[serde(rename = "latestVersion")]
    latest_version: Option<ClawHubVersion>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClawHubStats {
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    installs: u64,
    #[serde(default)]
    stars: u64,
    #[serde(default)]
    versions: u64,
}

#[derive(Debug, Deserialize)]
struct ClawHubVersion {
    version: String,
}

/// ClawHub API 分页响应
#[derive(Debug, Deserialize)]
struct ClawHubResponse {
    items: Vec<ClawHubSkill>,
    #[serde(rename = "nextCursor")]
    #[allow(dead_code)]
    next_cursor: Option<String>,
}

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
    let dir = skills_dir(app);
    if !dir.exists() {
        return String::new();
    }

    let mut prompts: Vec<String> = Vec::new();
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

/// 列出本地已安装的所有 Skills
#[tauri::command]
pub fn skills_list(app: AppHandle) -> Result<Vec<SkillInfo>, String> {
    let dir = skills_dir(&app);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
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

    // 按分类排序
    skills.sort_by(|a, b| a.category.cmp(&b.category).then(a.id.cmp(&b.id)));
    Ok(skills)
}

/// 启用或禁用一个 Skill
#[tauri::command]
pub fn skills_toggle(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let dir = skills_dir(&app).join(&id);
    if !dir.exists() {
        return Err(format!("Skill '{}' 未找到", id));
    }

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

/// 获取 Skills 市场列表（优先 ClawHub API → GitHub 搜索 → 本地 market.json）
#[tauri::command]
pub async fn skills_market_list() -> Result<Vec<SkillMarketEntry>, String> {
    let mut entries: Vec<SkillMarketEntry> = Vec::new();
    let mut has_clawhub = false;

    // 1. 尝试从 ClawHub API 获取市场数据
    let clawhub_url = format!("{CLAWHUB_API}?limit=30");
    match reqwest::get(&clawhub_url).await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<ClawHubResponse>().await {
                Ok(data) => {
                    has_clawhub = true;
                    for skill in data.items {
                        entries.push(clawhub_to_entry(&skill));
                    }
                    eprintln!("[skills] ClawHub: 获取到 {} 个技能", entries.len());
                }
                Err(e) => {
                    eprintln!("[skills] ClawHub 解析失败: {}", e);
                }
            }
        }
        Ok(resp) => {
            eprintln!("[skills] ClawHub 返回状态: {}", resp.status());
        }
        Err(e) => {
            eprintln!("[skills] ClawHub 请求失败: {}", e);
        }
    }

    // 2. GitHub 搜索补充（topic:codebuddy-skill + topic:claude-skill）
    match fetch_github_skills().await {
        Ok(github_skills) => {
            let existing_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
            let mut added = 0;
            for skill in github_skills {
                if !existing_ids.contains(&skill.id) {
                    entries.push(skill);
                    added += 1;
                }
            }
            if added > 0 {
                has_clawhub = true; // 有在线数据
                eprintln!("[skills] GitHub: {} 个新技能", added);
            }
        }
        Err(e) => {
            eprintln!("[skills] GitHub 搜索失败: {}", e);
        }
    }

    // 3. 合并本地 market.json（补充在线源中没有的技能）
    let local = if has_clawhub {
        match fetch_local_market().await {
            Ok(local_entries) => {
                // 只保留 ClawHub 中没有的技能
                let clawhub_ids: HashSet<String> =
                    entries.iter().map(|e| e.id.clone()).collect();
                local_entries
                    .into_iter()
                    .filter(|e| !clawhub_ids.contains(&e.id))
                    .collect()
            }
            Err(_) => Vec::new(),
        }
    } else {
        // ClawHub 不可用，直接用本地数据
        match fetch_local_market().await {
            Ok(local_entries) => local_entries,
            Err(_) => builtin_market(),
        }
    };

    entries.extend(local);

    // 按下载量降序排列
    entries.sort_by(|a, b| b.downloads.cmp(&a.downloads));

    Ok(entries)
}

/// 将 ClawHub API 条目转为统一的 SkillMarketEntry
fn clawhub_to_entry(skill: &ClawHubSkill) -> SkillMarketEntry {
    let stats = skill.stats.as_ref();
    let version = skill
        .latest_version
        .as_ref()
        .map(|v| v.version.clone())
        .unwrap_or_else(|| "0.1.0".into());

    // 尝试从 description 或 topic 推断分类
    let category = infer_category(&skill.slug, &skill.topics);

    // ClawHub 技能没有直接下载 URL，标记为 external_install
    let download_url = String::new();

    SkillMarketEntry {
        id: skill.slug.clone(),
        name: skill.display_name.clone(),
        description_zh: skill
            .description
            .clone()
            .unwrap_or_else(|| skill.summary.clone()),
        description_en: skill.summary.clone(),
        version,
        category,
        download_url,
        size_bytes: 0,
        installed: false,
        source: "clawhub".into(),
        downloads: stats.map(|s| s.downloads).unwrap_or(0),
        stars: stats.map(|s| s.stars).unwrap_or(0),
        external_install: true,
    }
}

/// 根据 slug 和 topics 推断分类
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
                        external_install: false,
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
        "https://raw.githubusercontent.com/346379/Agent/main/.codebuddy/skills/market.json";
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

/// 通过 ClawHub CLI 安装技能
#[tauri::command]
pub async fn skills_clawhub_install(id: String) -> Result<String, String> {
    // 尝试运行 clawhub CLI 安装
    let output = tokio::process::Command::new("clawhub")
        .args(["skill", "install", &id])
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

/// 从 GitHub 下载并安装一个 Skill
#[tauri::command]
pub async fn skills_install(app: AppHandle, id: String, download_url: String) -> Result<(), String> {
    let dir = skills_dir(&app).join(&id);

    // 如果已存在，先备份再覆盖
    if dir.exists() {
        let backup = skills_dir(&app).join(format!("{}.bak", id));
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(&dir, &backup).map_err(|e| format!("备份失败: {}", e))?;
    }

    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    // 下载 SKILL.md
    let skill_md_url = if download_url.ends_with('/') {
        format!("{}SKILL.md", download_url)
    } else {
        format!("{}/SKILL.md", download_url)
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(&skill_md_url)
        .send()
        .await
        .map_err(|e| format!("下载 SKILL.md 失败: {}", e))?;

    if !resp.status().is_success() {
        // 清理已创建的空目录
        let _ = fs::remove_dir_all(&dir);
        return Err(format!("SKILL.md 不存在 (HTTP {})", resp.status()));
    }

    let content = resp
        .text()
        .await
        .map_err(|e| format!("读取内容失败: {}", e))?;

    fs::write(dir.join("SKILL.md"), content).map_err(|e| format!("写入失败: {}", e))?;

    // 尝试下载可选的 README.md
    let readme_url = if download_url.ends_with('/') {
        format!("{}README.md", download_url)
    } else {
        format!("{}/README.md", download_url)
    };

    if let Ok(resp) = client.get(&readme_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                let _ = fs::write(dir.join("README.md"), text);
            }
        }
    }

    Ok(())
}

/// 删除本地 Skill
#[tauri::command]
pub fn skills_delete(app: AppHandle, id: String) -> Result<(), String> {
    let dir = skills_dir(&app).join(&id);
    if !dir.exists() {
        return Err(format!("Skill '{}' 未找到", id));
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
    let path = skills_dir(&app).join(&id).join("SKILL.md");
    if !path.exists() {
        return Err(format!("Skill '{}' 的 SKILL.md 不存在", id));
    }
    fs::read_to_string(&path).map_err(|e| format!("读取失败: {}", e))
}

// ===================== 内置市场列表 =====================

fn builtin_market() -> Vec<SkillMarketEntry> {
    let base = "https://raw.githubusercontent.com/346379/Agent/main/.codebuddy/skills";
    let local = || SkillMarketEntry {
        source: "local".into(),
        downloads: 0,
        stars: 0,
        external_install: false,
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
