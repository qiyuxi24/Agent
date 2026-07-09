// Skills 管理模块
// - 扫描本地 .codebuddy/skills/ 目录
// - 启用/禁用 skill（通过 .gitignore 标记）
// - 从 GitHub raw 下载 skill SKILL.md + 资源

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

/// Skills 根目录（相对于项目根目录的 .codebuddy/skills/）
const SKILLS_DIR: &str = ".codebuddy/skills";

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

/// Skills 市场的条目（从远程 JSON 获取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMarketEntry {
    pub id: String,
    pub name: String,
    pub description_zh: String,
    pub description_en: String,
    pub version: String,
    pub category: String,
    pub download_url: String,
    pub size_bytes: u64,
    /// 是否已安装
    #[serde(default)]
    pub installed: bool,
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

/// 获取 Skills 市场列表（从 GitHub 远程仓库获取）
/// 如果没有网络则返回内置的预定义列表
#[tauri::command]
pub async fn skills_market_list() -> Result<Vec<SkillMarketEntry>, String> {
    let market_url = "https://raw.githubusercontent.com/346379/Agent/main/.codebuddy/skills/market.json";

    // 尝试从远程获取
    match reqwest::get(market_url).await {
        Ok(resp) if resp.status().is_success() => {
            let entries: Vec<SkillMarketEntry> = resp
                .json()
                .await
                .map_err(|e| format!("解析市场列表失败: {}", e))?;
            return Ok(entries);
        }
        Ok(resp) => {
            eprintln!("[skills] 市场请求返回状态码: {}", resp.status());
        }
        Err(e) => {
            eprintln!("[skills] 获取市场列表失败: {}", e);
        }
    }

    // 回退：返回内置列表
    Ok(builtin_market())
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
    vec![
        SkillMarketEntry {
            id: "frontend-dev".into(),
            name: "前端开发工作室".into(),
            description_zh: "全栈前端开发：精美 UI 设计、电影级动画、AI 媒体生成、转化文案。构建落地页、营销网站、产品页、仪表盘。".into(),
            description_en: "Full-stack frontend: premium UI, cinematic animations, AI media, persuasive copy.".into(),
            version: "0.1.1".into(),
            category: "frontend".into(),
            download_url: format!("{}/frontend-dev", base),
            size_bytes: 14480,
            installed: false,
        },
        SkillMarketEntry {
            id: "fullstack-dev".into(),
            name: "全栈开发".into(),
            description_zh: "后端架构 + 前后端集成：Express/Next.js/FastAPI/Go，REST API，实时功能(SSE/WebSocket)，认证，文件上传。".into(),
            description_en: "Backend architecture & fullstack integration: REST APIs, real-time, auth, file uploads.".into(),
            version: "0.1.0".into(),
            category: "backend".into(),
            download_url: format!("{}/fullstack-dev", base),
            size_bytes: 13970,
            installed: false,
        },
        SkillMarketEntry {
            id: "mcp-builder".into(),
            name: "MCP 服务器构建器".into(),
            description_zh: "构建高质量 MCP 服务器：Python/Node SDK，工具设计，stdio 传输，让 LLM 调用外部服务。".into(),
            description_en: "Build MCP servers: Python/Node SDK, tool design, stdio transport, LLM ↔ external services.".into(),
            version: "0.1.0".into(),
            category: "mcp".into(),
            download_url: format!("{}/mcp-builder", base),
            size_bytes: 7500,
            installed: false,
        },
        SkillMarketEntry {
            id: "prompt-engineering-expert".into(),
            name: "Prompt 工程专家".into(),
            description_zh: "高级 Prompt 工程：自定义指令设计、提示词优化、AI Agent 行为调优。".into(),
            description_en: "Advanced prompt engineering: custom instructions, prompt optimization, AI agent tuning.".into(),
            version: "0.1.0".into(),
            category: "ai".into(),
            download_url: format!("{}/prompt-engineering-expert", base),
            size_bytes: 1630,
            installed: false,
        },
        SkillMarketEntry {
            id: "agent-team-orchestration".into(),
            name: "智能体团队编排".into(),
            description_zh: "多智能体团队编排：角色定义、任务流转、交接协议、质量门禁。".into(),
            description_en: "Multi-agent team orchestration: roles, task routing, handoff protocols, quality gates.".into(),
            version: "0.1.0".into(),
            category: "ai".into(),
            download_url: format!("{}/agent-team-orchestration", base),
            size_bytes: 5100,
            installed: false,
        },
        SkillMarketEntry {
            id: "deep-research".into(),
            name: "深度研究".into(),
            description_zh: "结构化深度研究：生成大纲、并行搜索、Markdown 报告。支持学术研究、技术选型、市场分析。".into(),
            description_en: "Structured deep research: outline generation, parallel search, Markdown reports.".into(),
            version: "0.1.0".into(),
            category: "research".into(),
            download_url: format!("{}/deep-research", base),
            size_bytes: 2325,
            installed: false,
        },
        SkillMarketEntry {
            id: "browser-use".into(),
            name: "浏览器自动化".into(),
            description_zh: "浏览器交互自动化：网页测试、表单填写、截图、数据提取。".into(),
            description_en: "Browser automation: web testing, form filling, screenshots, data extraction.".into(),
            version: "0.1.0".into(),
            category: "tools".into(),
            download_url: format!("{}/browser-use", base),
            size_bytes: 7760,
            installed: false,
        },
    ]
}
