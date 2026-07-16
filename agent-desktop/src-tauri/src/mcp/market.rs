//! MCP 市场：从 npm/GitHub 动态抓取 MCP Server 列表，含内置回退。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::time::{Duration, Instant};

/// MCP 市场条目（前端展示用）
#[derive(Debug, Clone, Serialize)]
pub struct McpMarketEntry {
    pub name: String,
    pub description: String,
    pub description_zh: String,
    pub command: String,
    pub args: String,
    pub category: String,
    pub stars: u64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
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
    html_url: String,
    topics: Option<Vec<String>>,
}

/// npm 搜索响应
#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmPackage>,
    #[allow(dead_code)]
    total: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    package: NpmPackageInfo,
    #[allow(dead_code)]
    score: Option<NpmScore>,
}

#[derive(Debug, Deserialize)]
struct NpmPackageInfo {
    name: String,
    #[allow(dead_code)]
    version: String,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    #[allow(dead_code)]
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

#[derive(Debug, Deserialize)]
struct NpmScore {
    #[allow(dead_code)]
    final_score: Option<f64>,
}

/// 市场数据缓存（避免频繁请求 API）
static MARKET_CACHE: std::sync::OnceLock<tokio::sync::Mutex<Option<(Instant, Vec<McpMarketEntry>)>>> =
    std::sync::OnceLock::new();

fn market_cache() -> &'static tokio::sync::Mutex<Option<(Instant, Vec<McpMarketEntry>)>> {
    MARKET_CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// 获取 MCP 市场列表（从 npm registry + GitHub 动态抓取）
#[tauri::command]
pub async fn mcp_market_list() -> Result<Vec<McpMarketEntry>, String> {
    // 检查缓存（5 分钟内有效）
    {
        let cache = market_cache().lock().await;
        if let Some((ts, entries)) = cache.as_ref() {
            if ts.elapsed() < Duration::from_secs(300) {
                eprintln!("[MCP市场] 缓存命中 ({} 条)", entries.len());
                return Ok(entries.clone());
            }
        }
    }

    let mut entries: Vec<McpMarketEntry> = Vec::new();
    let mut seen = HashSet::new();

    // 1. 从 npm registry 搜索 mcp-server 包
    match fetch_npm_mcp_packages().await {
        Ok(pkgs) => {
            for pkg in pkgs {
                let entry = npm_to_entry(&pkg);
                if seen.insert(entry.name.clone()) {
                    entries.push(entry);
                }
            }
            eprintln!("[MCP市场] npm: {} 个包", entries.len());
        }
        Err(e) => {
            eprintln!("[MCP市场] npm 请求失败: {}", e);
        }
    }

    // 2. 从 GitHub 搜索 topic:mcp 补充
    match fetch_github_mcp_repos().await {
        Ok(repos) => {
            let mut github_count = 0;
            for repo in repos {
                if let Some(entry) = github_to_entry(&repo) {
                    if seen.insert(entry.name.clone()) {
                        github_count += 1;
                        entries.push(entry);
                    }
                }
            }
            eprintln!("[MCP市场] GitHub: {} 个新条目", github_count);
        }
        Err(e) => {
            eprintln!("[MCP市场] GitHub 请求失败: {}", e);
        }
    }

    // 3. 回退到内置列表
    if entries.is_empty() {
        eprintln!("[MCP市场] 所有在线源失败，使用内置列表");
        entries = builtin_mcp_market();
    }

    entries.sort_by(|a, b| b.stars.cmp(&a.stars));

    {
        let mut cache = market_cache().lock().await;
        *cache = Some((Instant::now(), entries.clone()));
    }

    eprintln!("[MCP市场] 共 {} 个条目", entries.len());
    Ok(entries)
}

/// 从 npm registry 搜索 mcp-server 包
async fn fetch_npm_mcp_packages() -> Result<Vec<NpmPackageInfo>, String> {
    let urls = [
        "https://registry.npmjs.org/-/v1/search?text=keywords:mcp-server&size=50",
        "https://registry.npmjs.org/-/v1/search?text=keywords:mcp&size=30",
    ];

    let mut all_packages: Vec<NpmPackageInfo> = Vec::new();
    let client = crate::build_market_client(10)?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            eprintln!("[MCP市场] npm {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<NpmSearchResponse>().await {
            Ok(data) => {
                for obj in data.objects {
                    let name = &obj.package.name;
                    if name.contains("mcp-server") || name.contains("mcp_server") {
                        all_packages.push(obj.package);
                    }
                }
            }
            Err(e) => {
                eprintln!("[MCP市场] npm 解析失败: {}", e);
            }
        }
    }

    Ok(all_packages)
}

/// 从 GitHub 搜索 MCP 相关仓库
async fn fetch_github_mcp_repos() -> Result<Vec<GitHubRepo>, String> {
    let urls = [
        "https://api.github.com/search/repositories?q=topic:mcp-server&sort=stars&order=desc&per_page=30",
        "https://api.github.com/search/repositories?q=mcp+server+in:name&sort=stars&order=desc&per_page=20",
    ];

    let mut all_repos: Vec<GitHubRepo> = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"));
            h
        })
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;

    for url in &urls {
        let resp = client.get(*url).send().await.map_err(|e| e.to_string())?;
        if resp.status() == 403 {
            eprintln!("[MCP市场] GitHub API 限流，跳过");
            continue;
        }
        if !resp.status().is_success() {
            eprintln!("[MCP市场] GitHub {} 返回 {}", url, resp.status());
            continue;
        }
        match resp.json::<GitHubSearchResponse>().await {
            Ok(data) => all_repos.extend(data.items),
            Err(e) => eprintln!("[MCP市场] GitHub 解析失败: {}", e),
        }
    }

    Ok(all_repos)
}

/// npm 包 → 市场条目
fn npm_to_entry(pkg: &NpmPackageInfo) -> McpMarketEntry {
    let category = infer_mcp_category(&pkg.name, pkg.keywords.as_deref().unwrap_or(&[]));
    McpMarketEntry {
        name: pkg.name.clone(),
        description: pkg.description.clone().unwrap_or_default(),
        description_zh: String::new(),
        command: "npx".into(),
        args: format!("-y {}", pkg.name),
        category,
        stars: 0,
        source: "npm".into(),
        env: infer_mcp_env(&pkg.name),
        homepage: None,
    }
}

/// GitHub 仓库 → 市场条目
fn github_to_entry(repo: &GitHubRepo) -> Option<McpMarketEntry> {
    let name = repo.full_name.clone();
    if name == "modelcontextprotocol/servers" {
        return None;
    }

    let npm_pkg = if name.contains("mcp-server") {
        let org = name.split('/').next()?;
        let pkg = name.split('/').nth(1)?;
        format!("@{}//{}", org, pkg)
    } else {
        return None;
    };

    let npm_pkg = npm_pkg.replace("//", "/");
    let desc = repo.description.clone().unwrap_or_default();
    let category = infer_mcp_category(&npm_pkg, repo.topics.as_deref().unwrap_or(&[]));

    Some(McpMarketEntry {
        name: npm_pkg.clone(),
        description: desc.clone(),
        description_zh: String::new(),
        command: "npx".into(),
        args: format!("-y {}", npm_pkg),
        category,
        stars: repo.stargazers_count,
        source: "github".into(),
        env: infer_mcp_env(&npm_pkg),
        homepage: Some(repo.html_url.clone()),
    })
}

/// 根据包名和关键词推断分类
fn infer_mcp_category(name: &str, keywords: &[String]) -> String {
    let lower = name.to_lowercase();
    let kw_lower: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();

    if lower.contains("file") || lower.contains("fs") || kw_lower.iter().any(|k| k.contains("file")) {
        "tools".into()
    } else if lower.contains("github") || lower.contains("git") || kw_lower.iter().any(|k| k.contains("git")) {
        "tools".into()
    } else if lower.contains("search") || lower.contains("brave") || lower.contains("tavily") || kw_lower.iter().any(|k| k.contains("search")) {
        "search".into()
    } else if lower.contains("puppeteer") || lower.contains("playwright") || lower.contains("browser") || lower.contains("chrome") || kw_lower.iter().any(|k| k.contains("browser")) {
        "browser".into()
    } else if lower.contains("postgres") || lower.contains("sqlite") || lower.contains("mysql") || lower.contains("database") || lower.contains("qdrant") || lower.contains("redis") || kw_lower.iter().any(|k| k.contains("database") || k.contains("sql")) {
        "database".into()
    } else if lower.contains("memory") || lower.contains("think") || lower.contains("reason") || lower.contains("ai") || lower.contains("llm") || kw_lower.iter().any(|k| k.contains("ai") || k.contains("memory")) {
        "ai".into()
    } else if lower.contains("slack") || lower.contains("notion") || lower.contains("linear") || lower.contains("jira") || kw_lower.iter().any(|k| k.contains("communication")) {
        "communication".into()
    } else if lower.contains("figma") || lower.contains("design") || lower.contains("map") || kw_lower.iter().any(|k| k.contains("design")) {
        "design".into()
    } else if lower.contains("docker") || lower.contains("sentry") || lower.contains("cloudflare") || lower.contains("k8s") || lower.contains("kubernetes") || kw_lower.iter().any(|k| k.contains("infra")) {
        "infra".into()
    } else if lower.contains("image") || lower.contains("replicate") || lower.contains("everart") || kw_lower.iter().any(|k| k.contains("image") || k.contains("generation")) {
        "ai".into()
    } else {
        "tools".into()
    }
}

/// 根据包名推断需要的环境变量
fn infer_mcp_env(pkg_name: &str) -> Option<HashMap<String, String>> {
    let lower = pkg_name.to_lowercase();
    let mut env = HashMap::new();

    if lower.contains("github") {
        env.insert("GITHUB_PERSONAL_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("brave") {
        env.insert("BRAVE_API_KEY".into(), "".into());
    } else if lower.contains("tavily") {
        env.insert("TAVILY_API_KEY".into(), "".into());
    } else if lower.contains("postgres") {
        env.insert("DATABASE_URL".into(), "postgresql://localhost:5432/...".into());
    } else if lower.contains("slack") {
        env.insert("SLACK_BOT_TOKEN".into(), "".into());
    } else if lower.contains("notion") {
        env.insert("NOTION_API_KEY".into(), "".into());
    } else if lower.contains("linear") {
        env.insert("LINEAR_API_KEY".into(), "".into());
    } else if lower.contains("figma") {
        env.insert("FIGMA_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("sentry") {
        env.insert("SENTRY_AUTH_TOKEN".into(), "".into());
    } else if lower.contains("cloudflare") {
        env.insert("CLOUDFLARE_API_TOKEN".into(), "".into());
    } else if lower.contains("supabase") {
        env.insert("SUPABASE_ACCESS_TOKEN".into(), "".into());
    } else if lower.contains("everart") {
        env.insert("EVERART_API_KEY".into(), "".into());
    } else if lower.contains("replicate") {
        env.insert("REPLICATE_API_TOKEN".into(), "".into());
    } else if lower.contains("browserbase") {
        env.insert("BROWSERBASE_API_KEY".into(), "".into());
        env.insert("BROWSERBASE_PROJECT_ID".into(), "".into());
    } else {
        return None;
    }

    Some(env)
}

// ===================== 前置依赖检测 =====================

/// 检测指定命令是否可用（如 node、npx、uvx）
async fn check_command_available(command: &str) -> Result<String, String> {
    let output = tokio::process::Command::new(command)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("{} 未安装或不在 PATH 中 ({})", command, e))?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(version)
    } else {
        Err(format!("{} 命令返回非零退出码", command))
    }
}

/// 检测 MCP 连接所需的前置依赖是否就绪
#[tauri::command]
pub async fn mcp_check_prereq(command: String) -> Result<Vec<String>, String> {
    let mut details = Vec::new();
    let cmds: Vec<&str> = command.split_whitespace().collect();
    let base_cmd = cmds.first().copied().unwrap_or("");

    match check_command_available(base_cmd).await {
        Ok(version) => details.push(format!("✓ {} ({})", base_cmd, version)),
        Err(e) => details.push(format!("✗ {}", e)),
    }

    if base_cmd == "npx" || base_cmd == "npx.cmd" {
        match check_command_available("node").await {
            Ok(version) => details.push(format!("✓ node ({})", version)),
            Err(e) => details.push(format!("✗ {}", e)),
        }
    }

    Ok(details)
}

/// 内置 MCP 市场列表（离线回退用）
fn builtin_mcp_market() -> Vec<McpMarketEntry> {
    vec![
        McpMarketEntry { name: "@modelcontextprotocol/server-filesystem".into(), description: "File system operations: read/write files, create dirs, search files.".into(), description_zh: "文件系统操作：读写文件、创建目录、搜索文件、编辑代码。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-filesystem .".into(), category: "tools".into(), stars: 500, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-github".into(), description: "GitHub API integration: repos, issues, PRs, search code.".into(), description_zh: "GitHub API：管理仓库、Issue、PR、搜索代码。需 GITHUB_PERSONAL_ACCESS_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-github".into(), category: "tools".into(), stars: 400, source: "builtin".into(), env: Some(HashMap::from([("GITHUB_PERSONAL_ACCESS_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-git".into(), description: "Git version control: commit, branch, log, diff, blame.".into(), description_zh: "Git 版本控制：提交、分支、日志、diff、blame。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-git --repository .".into(), category: "tools".into(), stars: 300, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-fetch".into(), description: "Web content fetching: convert URLs to Markdown for AI reading.".into(), description_zh: "网页抓取：将 URL 转为 Markdown 文本。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-fetch".into(), category: "tools".into(), stars: 250, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-puppeteer".into(), description: "Puppeteer browser automation: screenshots, scraping, form filling.".into(), description_zh: "Puppeteer 浏览器自动化：截图、抓取、表单填写。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-puppeteer".into(), category: "browser".into(), stars: 450, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-brave-search".into(), description: "Brave Search: web & news search. Free 2000/month. Needs BRAVE_API_KEY.".into(), description_zh: "Brave 搜索引擎：网页+新闻搜索，免费2000次/月。需 BRAVE_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-brave-search".into(), category: "search".into(), stars: 350, source: "builtin".into(), env: Some(HashMap::from([("BRAVE_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-postgres".into(), description: "PostgreSQL database: SQL queries, schema inspection. Needs DATABASE_URL.".into(), description_zh: "PostgreSQL 数据库：SQL查询、表结构查看。需 DATABASE_URL。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-postgres".into(), category: "database".into(), stars: 300, source: "builtin".into(), env: Some(HashMap::from([("DATABASE_URL".into(), "postgresql://localhost:5432/...".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-memory".into(), description: "Persistent memory with vector search for AI across conversations.".into(), description_zh: "持久化记忆：向量检索，跨对话记住关键信息。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-memory".into(), category: "ai".into(), stars: 400, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-sequential-thinking".into(), description: "Sequential thinking engine for complex reasoning.".into(), description_zh: "分步推理引擎：复杂问题逐步思考、假设检验。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-sequential-thinking".into(), category: "ai".into(), stars: 350, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-slack".into(), description: "Slack integration: send messages, read channels. Needs SLACK_BOT_TOKEN.".into(), description_zh: "Slack 集成：发送消息、读取频道。需 SLACK_BOT_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-slack".into(), category: "communication".into(), stars: 200, source: "builtin".into(), env: Some(HashMap::from([("SLACK_BOT_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-notion".into(), description: "Notion workspace: pages, databases, comments. Needs NOTION_API_KEY.".into(), description_zh: "Notion 工作空间：页面、数据库、评论。需 NOTION_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-notion".into(), category: "communication".into(), stars: 180, source: "builtin".into(), env: Some(HashMap::from([("NOTION_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-linear".into(), description: "Linear project management: issues, projects. Needs LINEAR_API_KEY.".into(), description_zh: "Linear 项目管理：Issue、项目跟踪。需 LINEAR_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-linear".into(), category: "communication".into(), stars: 150, source: "builtin".into(), env: Some(HashMap::from([("LINEAR_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-figma".into(), description: "Figma design integration. Needs FIGMA_ACCESS_TOKEN.".into(), description_zh: "Figma 设计集成。需 FIGMA_ACCESS_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-figma".into(), category: "design".into(), stars: 120, source: "builtin".into(), env: Some(HashMap::from([("FIGMA_ACCESS_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-google-maps".into(), description: "Google Maps: places, directions, geocoding. Needs GOOGLE_MAPS_API_KEY.".into(), description_zh: "Google Maps：地点搜索、路线、地理编码。需 GOOGLE_MAPS_API_KEY。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-google-maps".into(), category: "design".into(), stars: 100, source: "builtin".into(), env: Some(HashMap::from([("GOOGLE_MAPS_API_KEY".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-sentry".into(), description: "Sentry error monitoring: errors, issues, performance. Needs SENTRY_AUTH_TOKEN.".into(), description_zh: "Sentry 错误监控：查询错误、跟踪Issue。需 SENTRY_AUTH_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-sentry".into(), category: "infra".into(), stars: 90, source: "builtin".into(), env: Some(HashMap::from([("SENTRY_AUTH_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-docker".into(), description: "Docker container management. Needs local Docker.".into(), description_zh: "Docker 容器管理：列表、启停、日志。需本地 Docker。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-docker".into(), category: "infra".into(), stars: 80, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-cloudflare".into(), description: "Cloudflare services: Workers, KV, R2, D1. Needs CLOUDFLARE_API_TOKEN.".into(), description_zh: "Cloudflare：Workers、KV、R2、D1。需 CLOUDFLARE_API_TOKEN。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-cloudflare".into(), category: "infra".into(), stars: 70, source: "builtin".into(), env: Some(HashMap::from([("CLOUDFLARE_API_TOKEN".into(), "".into())])), homepage: None },
        McpMarketEntry { name: "@modelcontextprotocol/server-everything".into(), description: "MCP reference server with all standard features demo.".into(), description_zh: "MCP 参考服务器：演示所有标准功能。学习MCP最佳实践。".into(), command: "npx".into(), args: "-y @modelcontextprotocol/server-everything".into(), category: "tools".into(), stars: 60, source: "builtin".into(), env: None, homepage: None },
        McpMarketEntry { name: "@executeautomation/playwright-mcp-server".into(), description: "Playwright multi-browser automation: Chromium, Firefox, WebKit.".into(), description_zh: "Playwright 多浏览器自动化(Chromium/Firefox/WebKit)。".into(), command: "npx".into(), args: "-y @executeautomation/playwright-mcp-server".into(), category: "browser".into(), stars: 200, source: "builtin".into(), env: None, homepage: None },
    ]
}
