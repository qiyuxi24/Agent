//! MCP 错误码表 — 用于工具调用链路中的结构化错误传递
//!
//! 设计原则：
//!   - 每个错误码对应一个明确的故障类别
//!   - Rust → 前端通过事件 payload 传递 (code + message)
//!   - 前端根据 code 决定降级策略（重试 / 降级 / 展示）
//!
//! # 错误码表
//!
//! | 错误码  | 类别            | 含义                         | 建议处理                 |
//! |---------|-----------------|------------------------------|--------------------------|
//! | MCP-001 | TIMEOUT         | 工具调用超时（单次超过阈值） | 让 LLM 知道失败，可能重试 |
//! | MCP-002 | PROCESS_EXITED  | MCP 子进程已退出/崩溃        | 通知用户，需要重新连接   |
//! | MCP-003 | TOOL_ERROR      | 工具执行逻辑错误             | 将错误传回 LLM，尝试修正 |
//! | MCP-004 | CONN_CLOSED     | MCP 连接已关闭               | 重新连接                 |
//! | MCP-005 | SERVER_NOT_FOUND| 指定的 MCP 服务器未连接      | 提示用户连接             |
//! | MCP-006 | NAME_FORMAT     | 工具名格式错误（应 server::tool） | 开发阶段 bug        |
//! | MCP-007 | ARGS_PARSE      | 工具参数 JSON 解析失败       | 将错误传回 LLM，尝试修正 |
//! | MCP-008 | IO_ERROR        | stdin/stdout 通信错误        | 重新连接                 |
//! | MCP-009 | JSON_PARSE      | JSON-RPC 响应解析失败        | 记录日志，可能重试       |
//! | MCP-010 | PROCESS_SPAWN   | MCP 进程启动失败             | 检查 command/path/config |
//! | MCP-011 | INIT_FAILED     | MCP initialize 握手失败      | 检查 MCP Server 兼容性  |
//! | MCP-012 | LLM_NETWORK     | LLM API 网络请求失败         | 检查网络/API Key         |
//! | MCP-013 | LLM_API_ERROR   | LLM API 返回错误状态码       | 检查 API Key/配额        |
//! | MCP-014 | LLM_STREAM_ERR  | LLM 流式读取中断             | 部分内容应已显示         |

use serde::Serialize;

/// 标准的 MCP 错误信息，可序列化传递给前端
#[derive(Debug, Clone, Serialize)]
pub struct McpError {
    /// 错误码，如 "MCP-001"
    pub code: &'static str,
    /// 分类标识，如 "TIMEOUT"
    pub category: &'static str,
    /// 面向用户的错误描述
    pub message: String,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl McpError {
    // ─── 工厂方法 ───

    pub fn timeout(detail: &str) -> Self {
        Self {
            code: "MCP-001",
            category: "TIMEOUT",
            message: format!("工具调用超时: {}", detail),
        }
    }

    pub fn process_exited(name: &str) -> Self {
        Self {
            code: "MCP-002",
            category: "PROCESS_EXITED",
            message: format!("MCP 进程已退出: {}", name),
        }
    }

    pub fn tool_error(msg: &str) -> Self {
        Self {
            code: "MCP-003",
            category: "TOOL_ERROR",
            message: format!("工具执行错误: {}", msg),
        }
    }

    pub fn conn_closed() -> Self {
        Self {
            code: "MCP-004",
            category: "CONN_CLOSED",
            message: "MCP 连接已关闭".to_string(),
        }
    }

    pub fn server_not_found(name: &str) -> Self {
        Self {
            code: "MCP-005",
            category: "SERVER_NOT_FOUND",
            message: format!("MCP 服务器未连接: {}", name),
        }
    }

    pub fn name_format() -> Self {
        Self {
            code: "MCP-006",
            category: "NAME_FORMAT",
            message: "工具名格式错误（应为 server::tool）".to_string(),
        }
    }

    pub fn args_parse(detail: &str) -> Self {
        Self {
            code: "MCP-007",
            category: "ARGS_PARSE",
            message: format!("参数解析失败: {}", detail),
        }
    }

    pub fn io_error(detail: &str) -> Self {
        Self {
            code: "MCP-008",
            category: "IO_ERROR",
            message: format!("通信错误: {}", detail),
        }
    }

    pub fn json_parse(detail: &str) -> Self {
        Self {
            code: "MCP-009",
            category: "JSON_PARSE",
            message: format!("JSON 解析失败: {}", detail),
        }
    }

    pub fn process_spawn(command: &str, detail: &str) -> Self {
        Self {
            code: "MCP-010",
            category: "PROCESS_SPAWN",
            message: format!("启动 MCP 进程失败 ({}): {}", command, detail),
        }
    }

    pub fn init_failed(detail: &str) -> Self {
        Self {
            code: "MCP-011",
            category: "INIT_FAILED",
            message: format!("MCP 初始化失败: {}", detail),
        }
    }

    pub fn llm_network(detail: &str) -> Self {
        Self {
            code: "MCP-012",
            category: "LLM_NETWORK",
            message: format!("LLM API 网络错误: {}", detail),
        }
    }

    pub fn llm_api_error(status: u16, detail: &str) -> Self {
        Self {
            code: "MCP-013",
            category: "LLM_API_ERROR",
            message: format!("LLM API 错误 ({}): {}", status, detail),
        }
    }

    pub fn llm_stream_err(detail: &str) -> Self {
        Self {
            code: "MCP-014",
            category: "LLM_STREAM_ERR",
            message: format!("流式读取错误: {}", detail),
        }
    }

    /// 是否属于可重试的错误（TIMEOUT、NETWORK 类）
    pub fn is_retryable(&self) -> bool {
        matches!(self.code, "MCP-001" | "MCP-012" | "MCP-014")
    }

    /// 是否需要前端提示用户重新连接
    pub fn needs_reconnect(&self) -> bool {
        matches!(self.code, "MCP-002" | "MCP-004" | "MCP-005" | "MCP-010" | "MCP-011")
    }
}
