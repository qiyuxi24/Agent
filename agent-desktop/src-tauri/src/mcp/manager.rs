//! MCP 管理器：聚合多个 MCP Server，提供工具调用、缓存、健康检查。

use super::client::McpClient;
use super::types::*;
use crate::error_codes::McpError;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

/// 管理所有已连接的 MCP Server，并聚合它们的工具供 LLM 使用
pub struct McpManager {
    pub servers: Mutex<HashMap<String, McpClient>>,
    /// 持久化服务器配置（用于崩溃后自动重连）
    server_configs: Mutex<HashMap<String, McpServerConfig>>,
    /// 工具调用缓存（key → 过期时间 + 结果），避免 LLM 重复调用
    tool_cache: Mutex<HashMap<String, (Instant, String)>>,
    /// llm_tools() 调用结果缓存（带 TTL），减少重复构建
    tools_cache: Mutex<Option<(Instant, Vec<Value>)>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            server_configs: Mutex::new(HashMap::new()),
            tool_cache: Mutex::new(HashMap::new()),
            tools_cache: Mutex::new(None),
        }
    }

    /// 连接（或重连）一个 MCP Server，同时保存配置用于自动重连
    pub async fn connect(&self, config: McpServerConfig) -> Result<usize, McpError> {
        let client = McpClient::connect(&config).await?;
        let count = client.tools.len();
        let name = config.name.clone();

        self.server_configs.lock().await.insert(name.clone(), config);

        let mut servers = self.servers.lock().await;
        if let Some(mut old) = servers.remove(&name) {
            old.kill();
        }
        servers.insert(name, client);
        drop(servers);
        self.invalidate_tools_cache().await;
        Ok(count)
    }

    /// 断开指定服务器（同时清理配置）
    pub async fn disconnect(&self, name: &str) -> Result<(), String> {
        self.server_configs.lock().await.remove(name);
        let mut servers = self.servers.lock().await;
        if let Some(mut c) = servers.remove(name) {
            c.kill();
        }
        drop(servers);
        self.invalidate_tools_cache().await;
        Ok(())
    }

    /// 重连指定服务器（使用之前保存的配置）
    pub async fn reconnect(&self, name: &str) -> Result<usize, McpError> {
        let config = {
            let configs = self.server_configs.lock().await;
            configs
                .get(name)
                .cloned()
                .ok_or_else(|| McpError::server_not_found(name))?
        };
        eprintln!("[MCP:{}] 自动重连...", name);
        self.connect(config).await
    }

    /// 列出已连接服务器信息
    pub async fn list_servers(&self) -> Vec<McpServerInfo> {
        let servers = self.servers.lock().await;
        servers
            .iter()
            .map(|(name, c)| McpServerInfo {
                name: name.clone(),
                status: "connected".to_string(),
                tool_count: c.tools.len(),
                resolved_command: c.resolved_command.clone(),
                error_count: c.error_count,
                last_error: c.last_error.clone(),
            })
            .collect()
    }

    /// 获取指定服务器的 stderr 日志（从共享环缓冲区读取）
    pub async fn get_stderr(&self, name: &str) -> Vec<String> {
        let servers = self.servers.lock().await;
        servers
            .get(name)
            .and_then(|c| c.stderr_ring.lock().ok())
            .map(|r| r.snapshot())
            .unwrap_or_default()
    }

    /// 生成给 LLM 的 tools 数组（工具名用 `server::tool` 命名空间避免冲突）
    /// 结果带 30s TTL 缓存
    pub async fn llm_tools(&self) -> Vec<Value> {
        {
            let cache = self.tools_cache.lock().await;
            if let Some((ts, tools)) = cache.as_ref() {
                if ts.elapsed() < Duration::from_secs(TOOLS_CACHE_TTL_SECS) {
                    return tools.clone();
                }
            }
        }

        let servers = self.servers.lock().await;
        let mut out = Vec::new();
        for (sname, client) in servers.iter() {
            for tool in &client.tools {
                let ns = format!("{}::{}", sname, tool.name);
                out.push(json!({
                    "type": "function",
                    "function": {
                        "name": ns,
                        "description": tool.description.clone().unwrap_or_default(),
                        "parameters": tool.input_schema,
                    }
                }));
            }
        }

        self.tools_cache.lock().await.replace((Instant::now(), out.clone()));
        out
    }

    /// 使 tools 缓存失效（在 servers 变更时调用）
    pub(crate) async fn invalidate_tools_cache(&self) {
        self.tools_cache.lock().await.take();
    }

    /// 构建缓存 key（server::tool + 序列化参数）
    fn cache_key(server: &str, tool: &str, args: &Value) -> String {
        format!(
            "{}::{}::{}",
            server,
            tool,
            serde_json::to_string(args).unwrap_or_default()
        )
    }

    /// 通过命名空间工具名调用对应 Server 的工具（带缓存）
    ///
    /// 锁设计：获取 client 后立即释放 `servers` 锁，避免 `call_tool`（异步 I/O）期间阻塞其他操作。
    /// 通过 HashMap::remove/insert 暂移 client 而非借用，实现了零锁跨越 await。
    pub async fn call_namespaced(
        &self,
        namespaced: &str,
        arguments: &str,
    ) -> Result<String, McpError> {
        let (server, tool) = namespaced
            .split_once("::")
            .ok_or_else(McpError::name_format)?;

        let args: Value = serde_json::from_str(arguments)
            .map_err(|e| McpError::args_parse(&e.to_string()))?;

        // 缓存检查（不持 servers 锁）
        let key = Self::cache_key(server, tool, &args);
        {
            let cache = self.tool_cache.lock().await;
            if let Some((ts, result)) = cache.get(&key) {
                if ts.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                    eprintln!("[MCP:{}] 缓存命中: {}", server, tool);
                    return Ok(result.clone());
                }
            }
        }

        // 从 map 中移出 client → 释放 servers 锁（避免 await 期间阻塞）
        let mut client = {
            self.servers
                .lock()
                .await
                .remove(server)
                .ok_or_else(|| McpError::server_not_found(server))?
        };

        // 进程已退出 → 自动重连
        if !client.is_alive() {
            eprintln!("[MCP:{}] 进程已退出，尝试自动重连后重试...", server);
            client.kill();
            drop(client); // 丢弃旧 client，reconnect 会创建新的
            self.reconnect(server).await?;
            // 从 map 中取出重连后的 client
            client = {
                self.servers
                    .lock()
                    .await
                    .remove(server)
                    .ok_or_else(|| McpError::server_not_found(server))?
            };
        }

        // 执行工具调用（此时不持任何 servers 锁）
        let result = client.call_tool(tool, args.clone()).await;

        if result.is_err() && !client.is_alive() {
            eprintln!("[MCP:{}] 工具调用失败且进程已退出，可能为崩溃", server);
            client.stdin = None;
        }

        // 将 client 放回 map
        {
            let mut servers = self.servers.lock().await;
            servers.insert(server.to_string(), client);
        }

        // 成功时写入缓存
        if let Ok(ref text) = result {
            let mut cache = self.tool_cache.lock().await;
            Self::trim_cache(&mut cache);
            cache.insert(key, (Instant::now(), text.clone()));
        }

        result
    }

    /// 裁剪缓存（超过上限则清空）
    fn trim_cache(cache: &mut HashMap<String, (Instant, String)>) {
        if cache.len() > CACHE_MAX_ENTRIES {
            cache.retain(|_, (ts, _)| ts.elapsed() < Duration::from_secs(CACHE_TTL_SECS));
            if cache.len() > CACHE_MAX_ENTRIES {
                cache.clear();
            }
        }
    }

    /// 检查所有服务器的健康状态，返回已断开列表
    pub async fn health_check(&self, auto_reconnect: bool) -> Vec<String> {
        let mut servers = self.servers.lock().await;
        let mut dead = Vec::new();
        let names: Vec<String> = servers.keys().cloned().collect();
        for name in &names {
            let is_dead = servers
                .get_mut(name)
                .map(|c| !c.is_alive())
                .unwrap_or(true);
            if is_dead {
                dead.push(name.clone());
            }
        }
        for name in &dead {
            servers.remove(name);
            eprintln!("[MCP:{}] 健康检查发现进程已退出，已移除", name);
        }
        drop(servers);

        if auto_reconnect && !dead.is_empty() {
            for name in &dead {
                match self.reconnect(name).await {
                    Ok(n) => eprintln!("[MCP:{}] 自动重连成功，{} 个工具", name, n),
                    Err(e) => eprintln!("[MCP:{}] 自动重连失败: {}", name, e),
                }
            }
        }

        dead
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        self.tool_cache.lock().await.clear();
        eprintln!("[MCP] 工具调用缓存已清空");
    }

    /// 应用退出时清理所有 MCP 子进程
    pub async fn shutdown(&self) {
        let mut servers = self.servers.lock().await;
        if servers.is_empty() {
            return;
        }
        eprintln!("[MCP] 正在关闭 {} 个服务器...", servers.len());
        for (name, client) in servers.iter_mut() {
            eprintln!("[MCP] 终止子进程: {}", name);
            client.kill();
        }
        servers.clear();
        self.tool_cache.lock().await.clear();
        eprintln!("[MCP] 所有服务器已关闭");
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
