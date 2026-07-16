//! 中央工具中介层（Tool Registry）
//!
//! ## 设计目标
//! 聚合所有 AI Agent 可见的工具（MCP + 原生 + Skills）到统一注册表，
//! 以标准化格式（name / description / parameters）暴露给 LLM，
//! 并按名称自动分派到对应的执行后端。
//!
//! ## 业界参考
//! - **OpenAI function calling**：工具定义 = { type: "function", function: { name, description, parameters } }
//! - **Anthropic tool use**：工具定义 = { name, description, input_schema }
//! - **LangChain Tool**：Tool(name, description, func) 注册模式
//!
//! ## 命名空间约定
//! - MCP 工具：`serverName::toolName`（由 McpManager 维护）
//! - 原生工具：`native_<name>`（前缀避免与 MCP 冲突）
//! - Skills 工具：`skill::<name>`（预留，待 SKILL.md frontmatter 声明）

use crate::mcp::{McpManager};
use crate::rag::{RagManager, SearchQuery};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

// ============================================================
// 公共类型
// ============================================================

/// 工具来源标记（前端展示 + 调试用）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// 来自 MCP Server 的工具
    Mcp {
        server: String,
    },
    /// 内置原生工具（IDE 文件操作/代码执行/终端/RAG 等）
    Native,
    /// 来自 Skill 声明（预留）
    Skill,
}

/// 统一的工具定义（name + description + parameters，业界标准三元组）
///
/// 无论来源如何，每个工具都满足：
/// - `name`：LLM 调用的唯一标识
/// - `description`：语义说明，LLM 据此决定是否调用
/// - `parameters`：JSON Schema，LLM 据此填充参数
/// - `source`：来源标记，用于前端展示和调试
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub source: ToolSource,
}

impl ToolDefinition {
    /// 转为 OpenAI function-calling 格式（发给 LLM 的 tool 描述）
    pub fn to_openai_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// 原生工具的执行函数签名
///
/// 接收 JSON 字符串参数（owned String），返回 JSON 字符串结果或错误信息。
/// 使用 `String` 而非 `&str` 以避免闭包生命周期约束（必须满足 'static）。
pub type NativeToolFn =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

/// 已注册的原生工具（定义 + 执行器）
struct NativeToolEntry {
    def: ToolDefinition,
    exec: NativeToolFn,
}

// ============================================================
// ToolRegistry — 中央中介
// ============================================================

/// 中央工具注册表
///
/// 单一真相源，聚合 MCP 工具 + 原生工具 +（未来）Skills 工具，
/// 提供统一的 `all_tools()`（获取工具列表）和 `execute()`（调度执行）。
///
/// ## 使用方法
/// ```ignore
/// let registry = ToolRegistry::new();
/// registry.register_native(def, executor_fn);
/// let tools = registry.all_tools().await;  // → Vec<Value>
/// let result = registry.execute("native_read_file", r#"{"path":"/tmp/a.txt"}"#).await;
/// ```
pub struct ToolRegistry {
    mcp: McpManager,
    native_tools: Vec<NativeToolEntry>,
}

impl ToolRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        Self {
            mcp: McpManager::new(),
            native_tools: Vec::new(),
        }
    }

    // ---- MCP 子管理器访问 ----

    /// 获取 MCP 管理器引用（供现有的 MCP 管理命令使用）
    pub fn mcp(&self) -> &McpManager {
        &self.mcp
    }

    // ---- 原生工具注册 ----

    /// 注册一个原生工具
    pub fn register_native(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        exec: NativeToolFn,
    ) {
        self.native_tools.push(NativeToolEntry {
            def: ToolDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
                source: ToolSource::Native,
            },
            exec,
        });
    }

    /// 批量注册原生工具
    pub fn register_native_tools(&mut self, tools: Vec<(String, String, Value, NativeToolFn)>) {
        for (name, description, parameters, exec) in tools {
            self.register_native(name, description, parameters, exec);
        }
    }

    // ---- 工具集合（供 LLM 可见） ----

    /// 聚合所有工具 → OpenAI function-calling 格式
    ///
    /// 返回的 `Vec<Value>` 直接注入 `/chat/completions` 的 `tools` 字段，
    /// 让 LLM 可以看到所有可调用工具。
    pub async fn all_tools(&self) -> Vec<Value> {
        let mut all: Vec<Value> = Vec::new();

        // 1. MCP 工具（已由 McpManager 格式化为 OpenAI 格式）
        all.extend(self.mcp.llm_tools().await);

        // 2. 原生工具
        for nt in &self.native_tools {
            all.push(nt.def.to_openai_tool());
        }

        // 3. Skills 工具（预留）
        // TODO: 解析 SKILL.md frontmatter 中声明的工具定义

        all
    }

    // ---- 工具执行调度 ----

    /// 统一调度：按工具名自动分派到对应的执行后端
    ///
    /// 分派规则：
    /// - 名称含 `::` → MCP（格式：`serverName::toolName`）
    /// - 名称以 `native_` 开头 → 原生工具
    /// - 名称以 `skill::` 开头 → Skills 工具（预留）
    /// - 其他 → 依次尝试原生工具列表
    pub async fn execute(&self, name: &str, arguments: &str) -> Result<String, String> {
        // 判断是否是 MCP 工具（命名空间格式：server::tool）
        if name.contains("::") {
            return self
                .mcp
                .call_namespaced(name, arguments)
                .await
                .map_err(|e| e.to_string());
        }

        // 查找原生工具
        for nt in &self.native_tools {
            if nt.def.name == name {
                return (nt.exec)(arguments.to_string()).await;
            }
        }

        // 未找到
        let known = self.list_all_names().await;
        Err(format!(
            "未知工具 '{}'。可用工具：{}",
            name,
            known.join(", ")
        ))
    }

    /// 列出所有工具名称（调试/错误提示用）
    pub async fn list_all_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();

        // MCP 工具
        let tools = self.mcp.llm_tools().await;
        for t in &tools {
            if let Some(n) = t["function"]["name"].as_str() {
                names.push(n.to_string());
            }
        }

        // 原生工具
        for nt in &self.native_tools {
            names.push(nt.def.name.clone());
        }

        names.sort();
        names
    }

    /// 获取所有工具的 ToolDefinition（供前端展示/调试用）
    pub async fn list_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = Vec::new();

        // MCP 工具
        let servers = self.mcp.servers.lock().await;
        for (sname, client) in servers.iter() {
            for t in &client.tools {
                defs.push(ToolDefinition {
                    name: format!("{}::{}", sname, t.name),
                    description: t.description.clone().unwrap_or_default(),
                    parameters: t.input_schema.clone(),
                    source: ToolSource::Mcp {
                        server: sname.clone(),
                    },
                });
            }
        }
        drop(servers);

        // 原生工具
        for nt in &self.native_tools {
            defs.push(nt.def.clone());
        }

        defs
    }

    // ---- 生命周期 ----

    /// 关闭所有 MCP 子进程
    pub async fn shutdown(&self) {
        self.mcp.shutdown().await;
    }
}

// ============================================================
// 默认原生工具注册
// ============================================================

/// 构建默认的原生工具集（在 AppState 初始化时调用）
///
/// 包含：
/// - 文件操作：read_file / write_file / create_file / delete_file / rename_file / list_directory / search_files
/// - 代码执行：execute_code
/// - 终端命令：terminal_exec
/// - RAG 检索：rag_search
///
/// `rag_manager` 使用 Arc 以便在闭包中安全共享（RagManager 内部已是 Mutex 保护）。
pub fn default_native_tools(
    rag_manager: Arc<RagManager>,
) -> Vec<(String, String, Value, NativeToolFn)> {
    use crate::ide;

    let mut tools: Vec<(String, String, Value, NativeToolFn)> = Vec::new();

    // 1. read_file — 读取文件内容
    tools.push((
        "native_read_file".into(),
        "读取指定路径的文件内容。如果文件不存在或无法读取，返回错误信息。".into(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "文件的绝对路径" }
            },
            "required": ["path"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let path = v["path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'path' 参数".to_string())?;
                ide::ide_read_file(path.to_string()).await
            })
        }),
    ));

    // 2. write_file — 写入文件（创建或覆盖）
    tools.push((
        "native_write_file".into(),
        "写入内容到指定路径的文件。如果文件已存在则覆盖，如果目录不存在会自动创建。".into(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "文件的绝对路径" },
                "content": { "type": "string", "description": "要写入的文件内容" }
            },
            "required": ["path", "content"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let path = v["path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'path' 参数".to_string())?;
                let content = v["content"]
                    .as_str()
                    .ok_or_else(|| "缺少 'content' 参数".to_string())?;
                ide::ide_write_file(path.to_string(), content.to_string()).await?;
                Ok(format!("已写入 {} 字节到 {}", content.len(), path))
            })
        }),
    ));

    // 3. create_file — 创建文件或目录
    tools.push((
        "native_create_file".into(),
        "创建空文件或目录。如果父目录不存在会自动创建。".into(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "要创建的路径" },
                "is_dir": { "type": "boolean", "description": "是否为目录", "default": false }
            },
            "required": ["path"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let path = v["path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'path' 参数".to_string())?;
                let is_dir = v["is_dir"].as_bool().unwrap_or(false);
                ide::ide_create_file(path.to_string(), is_dir).await?;
                if is_dir {
                    Ok(format!("已创建目录: {}", path))
                } else {
                    Ok(format!("已创建文件: {}", path))
                }
            })
        }),
    ));

    // 4. delete_file — 删除文件或目录
    tools.push((
        "native_delete_file".into(),
        "删除指定路径的文件或目录。目录会递归删除。".into(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "要删除的路径" },
                "is_dir": { "type": "boolean", "description": "是否为目录", "default": false }
            },
            "required": ["path"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let path = v["path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'path' 参数".to_string())?;
                let is_dir = v["is_dir"].as_bool().unwrap_or(false);
                ide::ide_delete_file(path.to_string(), is_dir).await?;
                Ok(format!("已删除: {}", path))
            })
        }),
    ));

    // 5. rename_file — 重命名/移动
    tools.push((
        "native_rename_file".into(),
        "重命名文件或文件夹。也可用于移动（跨目录重命名）。".into(),
        json!({
            "type": "object",
            "properties": {
                "old_path": { "type": "string", "description": "原路径" },
                "new_path": { "type": "string", "description": "新路径" }
            },
            "required": ["old_path", "new_path"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let old = v["old_path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'old_path' 参数".to_string())?;
                let new = v["new_path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'new_path' 参数".to_string())?;
                ide::ide_rename_file(old.to_string(), new.to_string()).await?;
                Ok(format!("已重命名 {} → {}", old, new))
            })
        }),
    ));

    // 6. list_directory — 列出目录内容
    tools.push((
        "native_list_directory".into(),
        "列出指定目录的内容（文件和子目录）。支持显示隐藏文件。".into(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "目录的绝对路径" },
                "show_hidden": { "type": "boolean", "description": "是否显示隐藏文件", "default": false }
            },
            "required": ["path"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let path = v["path"]
                    .as_str()
                    .ok_or_else(|| "缺少 'path' 参数".to_string())?;
                let show_hidden = v["show_hidden"].as_bool();
                let entries = ide::ide_list_dir(path.to_string(), show_hidden).await?;
                let result: Vec<String> = entries
                    .iter()
                    .map(|e| {
                        let ty = if e.is_dir { "[DIR]".to_string() } else { format!("[{}B]", e.size) };
                        format!("{}  {}", ty, e.name)
                    })
                    .collect();
                Ok(result.join("\n"))
            })
        }),
    ));

    // 7. search_files — 搜索文件
    tools.push((
        "native_search_files".into(),
        "在指定目录中递归搜索包含关键字的文件。会搜索文件名和文件内容（限制 1MB 以下）。".into(),
        json!({
            "type": "object",
            "properties": {
                "dir": { "type": "string", "description": "搜索的根目录" },
                "query": { "type": "string", "description": "搜索关键字" },
                "case_sensitive": { "type": "boolean", "description": "是否区分大小写", "default": false }
            },
            "required": ["dir", "query"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let dir = v["dir"]
                    .as_str()
                    .ok_or_else(|| "缺少 'dir' 参数".to_string())?;
                let query = v["query"]
                    .as_str()
                    .ok_or_else(|| "缺少 'query' 参数".to_string())?;
                let case_sensitive = v["case_sensitive"].as_bool();
                let results = ide::ide_search_files(dir.to_string(), query.to_string(), case_sensitive).await?;
                if results.is_empty() {
                    return Ok(format!("未找到包含 '{}' 的文件", query));
                }
                let text = results.join("\n");
                Ok(format!("找到 {} 个文件:\n{}", results.len(), text))
            })
        }),
    ));

    // 8. execute_code — 执行代码
    tools.push((
        "native_execute_code".into(),
        "在沙盒中执行指定语言的一段代码。支持的语言：python, javascript, typescript, rust, go, c, cpp, bash, ruby, php。注意：代码执行有 30 秒超时限制。".into(),
        json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "description": "编程语言",
                    "enum": ["python", "javascript", "typescript", "rust", "go", "c", "cpp", "bash", "ruby", "php"]
                },
                "code": { "type": "string", "description": "要执行的源代码" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "编译参数（可选）" },
                "stdin": { "type": "string", "description": "标准输入内容（可选）" }
            },
            "required": ["language", "code"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let req: ide::ExecuteRequest =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let result = ide::ide_execute_code(req).await?;
                let mut out = String::new();
                if !result.stdout.is_empty() {
                    out.push_str(&format!("[stdout]\n{}\n", result.stdout));
                }
                if !result.stderr.is_empty() {
                    out.push_str(&format!("[stderr]\n{}\n", result.stderr));
                }
                out.push_str(&format!(
                    "[exit_code={}, elapsed={}ms{}]",
                    result.exit_code,
                    result.elapsed_ms,
                    if result.timed_out { ", TIMEOUT" } else { "" }
                ));
                Ok(out)
            })
        }),
    ));

    // 9. terminal_exec — 执行终端命令
    tools.push((
        "native_terminal_exec".into(),
        "在工作目录下执行一条终端命令（非交互式）。Windows 使用 PowerShell，其他系统使用 bash。有超时限制。".into(),
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "要执行的命令" },
                "cwd": { "type": "string", "description": "工作目录（可选，默认当前工作区）" }
            },
            "required": ["command"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let cmd = v["command"]
                    .as_str()
                    .ok_or_else(|| "缺少 'command' 参数".to_string())?;
                let cwd = v["cwd"].as_str().map(|s| s.to_string());
                let result = ide::ide_terminal_exec(cmd.to_string(), cwd).await?;
                let mut out = String::new();
                if !result.stdout.is_empty() {
                    out.push_str(&format!("[stdout]\n{}\n", result.stdout));
                }
                if !result.stderr.is_empty() {
                    out.push_str(&format!("[stderr]\n{}\n", result.stderr));
                }
                out.push_str(&format!("[exit_code={}]", result.exit_code));
                Ok(out)
            })
        }),
    ));

    // 10. rag_search — RAG 知识库检索
    tools.push((
        "native_rag_search".into(),
        "在知识库中搜索与查询相关的内容。知识库需要先初始化并索引文档后才能使用。".into(),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "搜索查询" },
                "top_k": { "type": "integer", "description": "返回结果数量", "default": 5 }
            },
            "required": ["query"]
        }),
        Arc::new(move |args: String| {
            let rag = rag_manager.clone();
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let query = v["query"]
                    .as_str()
                    .ok_or_else(|| "缺少 'query' 参数".to_string())?;
                let top_k = v["top_k"].as_u64().map(|n| n as usize);
                let search_query = SearchQuery {
                    query: query.to_string(),
                    top_k,
                    source_type_filter: None,
                };
                match rag.search(&search_query).await {
                    Ok(results) => {
                        if results.is_empty() {
                            return Ok("未找到相关结果".to_string());
                        }
                        let text: String = results
                            .into_iter()
                            .enumerate()
                            .map(|(i, r)| {
                                format!(
                                    "[{}] (score={:.4})\n{}\n\n",
                                    i + 1,
                                    r.score,
                                    r.content
                                )
                            })
                            .collect();
                        Ok(text)
                    }
                    Err(e) => Err(format!("RAG 检索失败: {}", e)),
                }
            })
        }),
    ));

    // 11. web_fetch — 抓取网页内容（原生实现，使用 reqwest，不依赖 MCP）
    tools.push((
        "native_web_fetch".into(),
        "抓取并读取指定 URL 的网页内容，返回纯文本（去除 HTML 标签）。支持 HTML/文本/JSON 类型。自带 30 秒超时。".into(),
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "要抓取的网页 URL" },
                "max_chars": { "type": "integer", "description": "最大返回字符数，默认 8000", "default": 8000 }
            },
            "required": ["url"]
        }),
        Arc::new(|args: String| {
            Box::pin(async move {
                let v: Value =
                    serde_json::from_str(&args).map_err(|e| format!("参数解析失败: {}", e))?;
                let url = v["url"]
                    .as_str()
                    .ok_or_else(|| "缺少 'url' 参数".to_string())?;
                let max_chars = v["max_chars"].as_u64().map(|n| n as usize).unwrap_or(8000);

                // 使用独立的 reqwest client（带超时，不走项目全局的 LazyLock）
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .user_agent(concat!("Mozilla/5.0 (compatible; Votek/", env!("CARGO_PKG_VERSION"), "; +https://github.com/346379/Agent)"))
                    .build()
                    .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

                let resp = client.get(url).send().await.map_err(|e| {
                    if e.is_timeout() {
                        "请求超时（30 秒），目标服务器无响应".to_string()
                    } else if e.is_connect() {
                        format!("无法连接到服务器: {e}")
                    } else {
                        format!("网络请求失败: {e}")
                    }
                })?;

                let status = resp.status();
                if !status.is_success() {
                    return Err(format!(
                        "HTTP {} {} — 无法获取该 URL（可能不存在或需要登录）",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("")
                    ));
                }

                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                // 非文本类型直接返回摘要
                if !content_type.contains("html")
                    && !content_type.contains("text")
                    && !content_type.contains("json")
                    && !content_type.contains("xml")
                {
                    let cl = resp.content_length().map(|n| n.to_string()).unwrap_or_else(|| "未知".into());
                    return Ok(format!(
                        "该 URL 返回的内容类型为 {}（大小约 {} 字节），无法直接以文本形式读取。",
                        content_type, cl
                    ));
                }

                let raw = resp.text().await.map_err(|e| format!("读取响应体失败: {e}"))?;

                // 根据内容类型处理
                let raw_text = if content_type.contains("html") {
                    strip_html_tags(&raw)
                } else {
                    raw
                };

                // 压缩空白
                let text = compress_whitespace(&raw_text);

                // 截断
                let result = if text.len() > max_chars {
                    let truncated = &text[..max_chars];
                    format!("{}…\n[已截断，原始长度 {} 字符]", truncated, text.len())
                } else {
                    text
                };

                if result.trim().is_empty() {
                    Ok("页面内容为空（可能是需要 JavaScript 渲染的单页应用）".to_string())
                } else {
                    Ok(result)
                }
            })
        }),
    ));

    tools
}

/// 去除 HTML 标签，返回纯文本
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script_style = false;
    let mut tag_name = String::new();

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_name.clear();
        } else if ch == '>' && in_tag {
            in_tag = false;
            let tn = tag_name.to_lowercase();
            in_script_style = tn == "script" || tn == "style";
            tag_name.clear();
        } else if in_tag {
            if ch != '/' && !ch.is_whitespace() {
                tag_name.push(ch);
            }
            // 处理自闭合标签
            if ch == '/' && tag_name.is_empty() {
                // skip
            }
        } else if !in_script_style {
            result.push(ch);
        }
    }

    // 解码基本 HTML 实体
    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ");

    result
}

/// 压缩多余空白：多空格→单空格，多换行→双换行
fn compress_whitespace(text: &str) -> String {
    // 先规范化所有空白
    let mut result = String::with_capacity(text.len());
    let mut prev_was_newline = false;
    let mut newline_count = 0u32;

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            newline_count += 1;
            if newline_count <= 2 {
                result.push('\n');
                prev_was_newline = true;
            }
        } else if ch.is_whitespace() {
            if !prev_was_newline && !result.ends_with(' ') {
                result.push(' ');
            }
            newline_count = 0;
            prev_was_newline = false;
        } else {
            result.push(ch);
            newline_count = 0;
            prev_was_newline = false;
        }
    }

    result.trim().to_string()
}
