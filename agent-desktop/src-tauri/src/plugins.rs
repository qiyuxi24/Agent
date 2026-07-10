// 插件管理模块（类 VS Code 扩展市场）
// - 本地插件存储在 .codebuddy/plugins/<id>/
// - 每个插件目录包含 plugin.json（元信息）+ 入口脚本/资源
// - 启用/禁用通过 .disabled 标记
// - 市场数据从 npm registry + GitHub API 在线抓取（5分钟缓存）

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

const PLUGINS_DIR: &str = ".codebuddy/plugins";

/// 插件元信息（写入 plugin.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub entry: Option<String>,
    pub contributes: Option<Vec<String>>,
}

/// 已安装插件（返回前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub enabled: bool,
    pub installed_at: Option<String>,
    pub entry: Option<String>,
    pub contributes: Option<Vec<String>>,
}

/// 插件市场条目（前端展示用，在线抓取）
#[derive(Debug, Clone, Serialize)]
pub struct PluginMarketEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub description_zh: String,
    pub category: String,
    pub stars: u64,
    pub source: String,
    pub download_url: Option<String>,
    pub homepage: Option<String>,
    pub contributes: Vec<String>,
}

// ===================== 文件系统辅助 =====================

fn plugins_dir(app: &AppHandle) -> PathBuf {
    let resource = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let base = resource
        .ancestors()
        .find(|p| p.join(PLUGINS_DIR).exists())
        .unwrap_or(&resource);
    let dir = base.join(PLUGINS_DIR);
    fs::create_dir_all(&dir).ok();
    dir
}

fn is_disabled(plugin_dir: &PathBuf) -> bool {
    plugin_dir.join(".disabled").exists()
}

fn set_disabled(plugin_dir: &PathBuf, disabled: bool) -> std::io::Result<()> {
    let path = plugin_dir.join(".disabled");
    if disabled {
        fs::write(&path, "")?;
    } else if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

fn read_plugin_meta(dir: &PathBuf) -> Option<PluginMeta> {
    let meta_path = dir.join("plugin.json");
    let content = fs::read_to_string(&meta_path).ok()?;
    serde_json::from_str::<PluginMeta>(&content).ok()
}

fn format_timestamp(secs: u64) -> String {
    use std::time::SystemTime;
    if let Ok(dur) = SystemTime::UNIX_EPOCH.elapsed() {
        let delta = dur.as_secs() - secs;
        if delta < 60 {
            format!("{}秒前", delta)
        } else if delta < 3600 {
            format!("{}分钟前", delta / 60)
        } else if delta < 86400 {
            format!("{}小时前", delta / 3600)
        } else {
            format!("{}天前", delta / 86400)
        }
    } else {
        String::new()
    }
}

// ===================== 已安装插件 CRUD =====================

/// 列出已安装的插件
#[tauri::command]
pub fn plugins_list(app: AppHandle) -> Result<Vec<InstalledPlugin>, String> {
    let dir = plugins_dir(&app);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let plugin_json = path.join("plugin.json");
            if !plugin_json.exists() {
                continue;
            }
            let meta = match read_plugin_meta(&path) {
                Some(m) => m,
                None => continue,
            };

            let disabled = is_disabled(&path);

            let installed_at = plugin_json
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_timestamp(d.as_secs()));

            result.push(InstalledPlugin {
                id: meta.id,
                name: meta.name,
                version: meta.version,
                author: meta.author,
                description: meta.description,
                category: meta.category,
                enabled: !disabled,
                installed_at,
                entry: meta.entry,
                contributes: meta.contributes,
            });
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// 安装插件：从 download_url 下载 zip 包解压到本地，或写入最小 plugin.json
#[tauri::command]
pub async fn plugins_install(
    app: AppHandle,
    plugin_id: String,
    download_url: Option<String>,
    plugin_name: Option<String>,
    plugin_version: Option<String>,
    plugin_author: Option<String>,
    plugin_description: Option<String>,
    plugin_category: Option<String>,
    plugin_contributes: Option<Vec<String>>,
) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    // 如果有 download_url，从远程下载并解压
    if let Some(ref url) = download_url {
        if !url.is_empty() {
            eprintln!("[插件] 下载 {} 从 {}", plugin_id, url);
            match download_and_extract_plugin(&plugin_dir, url).await {
                Ok(_) => {
                    eprintln!("[插件] {} 下载解压成功", plugin_id);
                    // 验证解压后是否有 plugin.json
                    if !plugin_dir.join("plugin.json").exists() {
                        // 解压后没有 plugin.json，写入传入的元信息
                        let meta = PluginMeta {
                            id: plugin_id.clone(),
                            name: plugin_name.unwrap_or_else(|| plugin_id.clone()),
                            version: plugin_version.unwrap_or_else(|| "0.1.0".into()),
                            author: plugin_author.unwrap_or_else(|| "Unknown".into()),
                            description: plugin_description
                                .unwrap_or_else(|| format!("Plugin: {}", plugin_id)),
                            category: plugin_category.unwrap_or_else(|| "other".into()),
                            entry: None,
                            contributes: plugin_contributes,
                        };
                        let json =
                            serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
                        fs::write(plugin_dir.join("plugin.json"), json)
                            .map_err(|e| e.to_string())?;
                    }
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("[插件] 下载失败，使用占位安装: {}", e);
                    // 下载失败时回退到占位安装
                }
            }
        }
    }

    // 占位安装：写入传入的元信息或默认值
    let meta = PluginMeta {
        id: plugin_id.clone(),
        name: plugin_name.unwrap_or_else(|| plugin_id.clone()),
        version: plugin_version.unwrap_or_else(|| "0.1.0".into()),
        author: plugin_author.unwrap_or_else(|| "Unknown".into()),
        description: plugin_description.unwrap_or_else(|| format!("Plugin: {}", plugin_id)),
        category: plugin_category.unwrap_or_else(|| "other".into()),
        entry: None,
        contributes: plugin_contributes,
    };

    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(plugin_dir.join("plugin.json"), json).map_err(|e| e.to_string())?;

    Ok(())
}

/// 从 URL 下载 zip 包并解压到目标目录
async fn download_and_extract_plugin(plugin_dir: &PathBuf, url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载返回 HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取响应体失败: {}", e))?;

    // 写入临时 zip 文件
    let tmp_zip = plugin_dir.join("_temp_download.zip");
    fs::write(&tmp_zip, &bytes).map_err(|e| format!("写入临时文件失败: {}", e))?;

    // 解压
    let file = fs::File::open(&tmp_zip).map_err(|e| format!("打开 zip 失败: {}", e))?;
    let reader = std::io::BufReader::new(file);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("解析 zip 失败: {}", e))?;

    // GitHub zipball 通常有一层顶层目录，需要跳过
    let mut has_top_dir = false;
    let mut top_dir_name = String::new();

    if archive.len() > 0 {
        let first = archive.by_index(0).map_err(|e| format!("读取 zip 条目失败: {}", e))?;
        let name = first.name().to_string();
        // 检查第一个条目是否为目录
        if name.ends_with('/') {
            has_top_dir = true;
            top_dir_name = name.trim_end_matches('/').to_string();
        }
    }

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("读取条目 {} 失败: {}", i, e))?;
        let name = entry.name().to_string();

        // 跳过顶层目录前缀
        let relative = if has_top_dir && name.starts_with(&top_dir_name) {
            let stripped = name[top_dir_name.len()..].trim_start_matches('/');
            if stripped.is_empty() {
                continue; // 跳过顶层目录本身
            }
            stripped.to_string()
        } else {
            name
        };

        let out_path = plugin_dir.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| format!("创建目录 {} 失败: {}", relative, e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建父目录失败: {}", e))?;
            }
            let mut outfile =
                fs::File::create(&out_path)
                    .map_err(|e| format!("创建文件 {} 失败: {}", relative, e))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("解压 {} 失败: {}", relative, e))?;
        }
    }

    // 清理临时文件
    let _ = fs::remove_file(&tmp_zip);

    Ok(())
}

/// 卸载插件
#[tauri::command]
pub fn plugins_delete(app: AppHandle, plugin_id: String) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 启用/禁用插件
#[tauri::command]
pub fn plugins_toggle(app: AppHandle, plugin_id: String, enabled: bool) -> Result<(), String> {
    let base = plugins_dir(&app);
    let plugin_dir = base.join(&plugin_id);
    if !plugin_dir.exists() {
        return Err(format!("Plugin '{}' not found", plugin_id));
    }
    set_disabled(&plugin_dir, !enabled).map_err(|e| e.to_string())
}

// ===================== 插件市场（在线抓取） =====================

/// npm 搜索响应（只取需要的字段）
#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmPackage>,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    package: NpmPackageInfo,
}

#[derive(Debug, Deserialize)]
struct NpmPackageInfo {
    name: String,
    version: String,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    #[serde(default)]
    links: Option<NpmLinks>,
}

#[derive(Debug, Deserialize)]
struct NpmLinks {
    #[allow(dead_code)]
    npm: Option<String>,
    #[allow(dead_code)]
    homepage: Option<String>,
    #[allow(dead_code)]
    repository: Option<String>,
}

/// GitHub 搜索响应
#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubRepo>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepo {
    full_name: String,
    description: Option<String>,
    stargazers_count: u64,
    #[allow(dead_code)]
    html_url: String,
    topics: Option<Vec<String>>,
}

/// 市场数据缓存（5分钟有效）
static PLUGIN_MARKET_CACHE: std::sync::OnceLock<
    tokio::sync::Mutex<Option<(tokio::time::Instant, Vec<PluginMarketEntry>)>>,
> = std::sync::OnceLock::new();

fn plugin_cache() -> &'static tokio::sync::Mutex<Option<(tokio::time::Instant, Vec<PluginMarketEntry>)>>
{
    PLUGIN_MARKET_CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// 获取插件市场列表（从 npm + GitHub 动态抓取）
#[tauri::command]
pub async fn plugin_market_list() -> Result<Vec<PluginMarketEntry>, String> {
    // 检查缓存（5分钟内有效）
    {
        let cache = plugin_cache().lock().await;
        if let Some((ts, entries)) = cache.as_ref() {
            if ts.elapsed() < std::time::Duration::from_secs(300) {
                eprintln!("[插件市场] 缓存命中 ({} 条)", entries.len());
                return Ok(entries.clone());
            }
        }
    }

    let mut entries: Vec<PluginMarketEntry> = Vec::new();
    let mut seen = HashSet::new();

    // 1. 从 npm registry 搜索
    match fetch_npm_plugins().await {
        Ok(pkgs) => {
            for pkg in pkgs {
                let entry = npm_to_plugin_entry(&pkg);
                if seen.insert(entry.id.clone()) {
                    entries.push(entry);
                }
            }
            eprintln!("[插件市场] npm: {} 个包", entries.len());
        }
        Err(e) => {
            eprintln!("[插件市场] npm 请求失败: {}", e);
        }
    }

    // 2. 从 GitHub 搜索
    match fetch_github_plugins().await {
        Ok(repos) => {
            let mut gh_count = 0;
            for repo in repos {
                if let Some(entry) = github_to_plugin_entry(&repo) {
                    if seen.insert(entry.id.clone()) {
                        gh_count += 1;
                        entries.push(entry);
                    }
                }
            }
            eprintln!("[插件市场] GitHub: {} 个新条目", gh_count);
        }
        Err(e) => {
            eprintln!("[插件市场] GitHub 请求失败: {}", e);
        }
    }

    // 3. 如果在线源完全失败，回退到内置列表
    if entries.is_empty() {
        eprintln!("[插件市场] 所有在线源失败，使用内置列表");
        entries = builtin_plugin_market();
    }

    // 按星标数降序排列
    entries.sort_by(|a, b| b.stars.cmp(&a.stars));

    // 写入缓存
    {
        let mut cache = plugin_cache().lock().await;
        *cache = Some((tokio::time::Instant::now(), entries.clone()));
    }

    eprintln!("[插件市场] 共 {} 个条目", entries.len());
    Ok(entries)
}

/// 从 npm registry 搜索插件包
async fn fetch_npm_plugins() -> Result<Vec<NpmPackageInfo>, String> {
    let urls = [
        "https://registry.npmjs.org/-/v1/search?text=keywords:agent-desktop-plugin&size=30",
        "https://registry.npmjs.org/-/v1/search?text=keywords:agent-plugin&size=30",
        "https://registry.npmjs.org/-/v1/search?text=keywords:codebuddy-plugin&size=20",
    ];

    let mut all_packages: Vec<NpmPackageInfo> = Vec::new();
    let mut seen = HashSet::new();
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            eprintln!("[插件市场] npm {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<NpmSearchResponse>().await {
            Ok(data) => {
                for obj in data.objects {
                    if seen.insert(obj.package.name.clone()) {
                        all_packages.push(obj.package);
                    }
                }
            }
            Err(e) => {
                eprintln!("[插件市场] npm 解析失败: {}", e);
            }
        }
    }

    Ok(all_packages)
}

/// 从 GitHub 搜索插件仓库
async fn fetch_github_plugins() -> Result<Vec<GitHubRepo>, String> {
    let urls = [
        "https://api.github.com/search/repositories?q=topic:agent-desktop-plugin&sort=stars&order=desc&per_page=30",
        "https://api.github.com/search/repositories?q=topic:codebuddy-plugin&sort=stars&order=desc&per_page=20",
    ];

    let mut all_repos: Vec<GitHubRepo> = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent("agent-desktop/0.3.0")
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
            );
            h
        })
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {}", e))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if resp.status() == 403 {
            eprintln!("[插件市场] GitHub API 限流，跳过");
            continue;
        }
        if !resp.status().is_success() {
            eprintln!("[插件市场] GitHub {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<GitHubSearchResponse>().await {
            Ok(data) => all_repos.extend(data.items),
            Err(e) => eprintln!("[插件市场] GitHub 解析失败: {}", e),
        }
    }

    Ok(all_repos)
}

/// npm 包 → 市场条目
fn npm_to_plugin_entry(pkg: &NpmPackageInfo) -> PluginMarketEntry {
    let keywords: Vec<String> = pkg.keywords.clone().unwrap_or_default();
    let category = infer_plugin_category(&pkg.name, &keywords);
    let download_url = infer_download_url(&pkg.name, None);
    let id = pkg.name.replace('@', "").replace('/', "-").trim_matches('-').to_string();

    PluginMarketEntry {
        id,
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        author: String::new(),
        description: pkg.description.clone().unwrap_or_default(),
        description_zh: String::new(),
        category,
        stars: 0,
        source: "npm".into(),
        download_url,
        homepage: pkg
            .links
            .as_ref()
            .and_then(|l| l.npm.clone()),
        contributes: match_contributes(&keywords),
    }
}

/// GitHub 仓库 → 市场条目
fn github_to_plugin_entry(repo: &GitHubRepo) -> Option<PluginMarketEntry> {
    let name = &repo.full_name;
    let desc = repo.description.clone().unwrap_or_default();
    let keywords: Vec<String> = repo.topics.clone().unwrap_or_default();

    // 从 repo 名生成 id
    let id = name.replace('/', "-");

    let category = infer_plugin_category(name, &keywords);
    let download_url = infer_download_url(name, Some(&repo.full_name));
    let contributes = match_contributes(&keywords);

    Some(PluginMarketEntry {
        id,
        name: name.clone(),
        version: "latest".into(),
        author: name.split('/').next().unwrap_or("Unknown").into(),
        description: desc.clone(),
        description_zh: String::new(),
        category,
        stars: repo.stargazers_count,
        source: "github".into(),
        download_url,
        homepage: Some(format!("https://github.com/{}", repo.full_name)),
        contributes,
    })
}

/// 根据包名/仓库名和关键词推断分类
fn infer_plugin_category(name: &str, keywords: &[String]) -> String {
    let lower = name.to_lowercase();
    let kw_lower: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();

    let all_text = format!("{} {}", lower, kw_lower.join(" "));

    if all_text.contains("theme") || all_text.contains("dark") || all_text.contains("light") {
        "theme".into()
    } else if all_text.contains("markdown") || all_text.contains("preview") || all_text.contains("render") {
        "tool".into()
    } else if all_text.contains("todo") || all_text.contains("task") || all_text.contains("kanban") || all_text.contains("笔记") {
        "productivity".into()
    } else if all_text.contains("chart") || all_text.contains("visual") || all_text.contains("graph") || all_text.contains("echarts") {
        "visualization".into()
    } else if all_text.contains("sync") || all_text.contains("obsidian") || all_text.contains("notion") || all_text.contains("github") || all_text.contains("gitlab") {
        "integration".into()
    } else if all_text.contains("code") || all_text.contains("editor") || all_text.contains("lint") || all_text.contains("format") {
        "tool".into()
    } else {
        "other".into()
    }
}

/// 根据关键词匹配贡献点
fn match_contributes(keywords: &[String]) -> Vec<String> {
    let mut contributes = Vec::new();
    for kw in keywords {
        let k = kw.to_lowercase();
        if k == "panel" && !contributes.contains(&"panel".to_string()) {
            contributes.push("panel".into());
        }
        if k == "command" && !contributes.contains(&"command".to_string()) {
            contributes.push("command".into());
        }
        if (k == "theme") && !contributes.contains(&"theme".to_string()) {
            contributes.push("theme".into());
        }
    }
    if contributes.is_empty() {
        contributes.push("panel".into()); // 默认归类为面板类
    }
    contributes
}

/// 从包名/仓库名推断下载地址（优先 GitHub Releases）
fn infer_download_url(_name: &str, full_name: Option<&str>) -> Option<String> {
    // 如果是 GitHub 仓库，构造 release 下载地址
    let repo = if let Some(repo_name) = full_name {
        repo_name.to_string()
    } else {
        return None;
    };

    // 尝试从 package.json 的 repository 字段解析，这里简化为直接使用 GitHub API
    Some(format!(
        "https://api.github.com/repos/{}/zipball/main",
        repo
    ))
}

/// 内置插件市场（离线回退用）
fn builtin_plugin_market() -> Vec<PluginMarketEntry> {
    vec![
        PluginMarketEntry {
            id: "theme-dark-pro".into(),
            name: "Dark Pro Theme".into(),
            version: "1.2.0".into(),
            author: "Agent Team".into(),
            description: "Professional dark theme with optimized code highlighting and contrast.".into(),
            description_zh: "专业深色主题，优化代码高亮与对比度，护眼且美观。".into(),
            category: "theme".into(),
            stars: 1200,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["theme".into()],
        },
        PluginMarketEntry {
            id: "markdown-preview".into(),
            name: "Markdown Preview".into(),
            version: "0.8.1".into(),
            author: "Agent Team".into(),
            description: "Sidebar live Markdown preview with GFM syntax, Mermaid diagrams, and MathJax.".into(),
            description_zh: "侧边栏实时 Markdown 预览，支持 GFM 语法、Mermaid 图表、数学公式。".into(),
            category: "tool".into(),
            stars: 850,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["panel".into()],
        },
        PluginMarketEntry {
            id: "todo-panel".into(),
            name: "Todo Panel".into(),
            version: "0.5.0".into(),
            author: "Agent Team".into(),
            description: "Built-in task management panel with Kanban view and AI task breakdown.".into(),
            description_zh: "内置任务管理面板，支持看板视图与 AI 自动拆解任务。".into(),
            category: "productivity".into(),
            stars: 720,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["panel".into()],
        },
        PluginMarketEntry {
            id: "chart-visualizer".into(),
            name: "Chart Visualizer".into(),
            version: "0.3.2".into(),
            author: "Agent Team".into(),
            description: "Render ECharts charts in conversations: line, bar, pie, scatter, and more.".into(),
            description_zh: "在对话中直接渲染 ECharts 图表，支持折线/柱状/饼图/散点图等。".into(),
            category: "visualization".into(),
            stars: 560,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["panel".into()],
        },
        PluginMarketEntry {
            id: "obsidian-sync".into(),
            name: "Obsidian Sync".into(),
            version: "0.2.0".into(),
            author: "Community".into(),
            description: "Two-way sync with Obsidian vault. Auto-archive conversations as Markdown notes.".into(),
            description_zh: "双向同步 Obsidian 笔记库，对话内容自动归档为 Markdown 笔记。".into(),
            category: "integration".into(),
            stars: 480,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["command".into()],
        },
        PluginMarketEntry {
            id: "github-copilot-chat".into(),
            name: "GitHub Copilot Chat".into(),
            version: "1.0.0".into(),
            author: "GitHub".into(),
            description: "Integrate GitHub Copilot Chat as alternative AI backend. Seamless switch between providers.".into(),
            description_zh: "接入 GitHub Copilot 对话服务，作为备选 AI 后端，支持无缝切换。".into(),
            category: "integration".into(),
            stars: 420,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["command".into()],
        },
        PluginMarketEntry {
            id: "code-formatter".into(),
            name: "Code Formatter".into(),
            version: "0.6.0".into(),
            author: "Agent Team".into(),
            description: "Auto-format code in multiple languages via Prettier, Black, rustfmt with customizable rules.".into(),
            description_zh: "多语言代码自动格式化，集成 Prettier/Black/rustfmt，支持自定义规则。".into(),
            category: "tool".into(),
            stars: 390,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["command".into(), "panel".into()],
        },
        PluginMarketEntry {
            id: "notion-exporter".into(),
            name: "Notion Exporter".into(),
            version: "0.4.1".into(),
            author: "Community".into(),
            description: "Export conversations and code snippets to Notion pages with templates support.".into(),
            description_zh: "导出对话和代码片段到 Notion 页面，支持模板。需 NOTION_API_KEY。".into(),
            category: "integration".into(),
            stars: 310,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["command".into()],
        },
        PluginMarketEntry {
            id: "spell-checker".into(),
            name: "Spell Checker".into(),
            version: "0.7.2".into(),
            author: "Agent Team".into(),
            description: "Real-time spell checking for Markdown and comments, multi-language dictionary support.".into(),
            description_zh: "Markdown 和注释的实时拼写检查，多语言词典支持。".into(),
            category: "tool".into(),
            stars: 280,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["panel".into()],
        },
        PluginMarketEntry {
            id: "sql-database-explorer".into(),
            name: "SQL Database Explorer".into(),
            version: "0.3.0".into(),
            author: "Agent Team".into(),
            description: "Browse, query, and visualize SQL databases. Supports SQLite, PostgreSQL, MySQL.".into(),
            description_zh: "浏览、查询和可视化 SQL 数据库。支持 SQLite、PostgreSQL、MySQL。".into(),
            category: "tool".into(),
            stars: 250,
            source: "builtin".into(),
            download_url: None,
            homepage: None,
            contributes: vec!["panel".into()],
        },
    ]
}
