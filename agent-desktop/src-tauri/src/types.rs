//! 核心类型定义：Chat 请求/消息、ToolCall、流事件等。
//!
//! 从 `lib.rs` 提取，避免 God Object 模式。
//! 通过 `pub use types::*` 在 crate 根重导出，保持 `use crate::ChatMessage` 等引用不变。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 前端发来的 Chat 请求
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,

    // ── Agent Loop 行为控制（从 Agent 配置透传） ──
    /// 工具使用行为：run_llm_again（默认）| stop_on_first_tool | stop_at_tools
    #[serde(default)]
    pub tool_use_behavior: String,
    /// stop_at_tools 模式下需停止的具体工具名列表
    #[serde(default)]
    pub stop_at_tool_names: Vec<String>,
    /// 需要人工审批的工具名列表
    #[serde(default)]
    pub require_tool_approval_for: Vec<String>,
    /// 最大循环轮次（0=使用服务端默认）
    #[serde(default)]
    pub max_iterations: usize,
    /// 工具结果富化阈值（字符数，0=禁用）
    #[serde(default)]
    pub enrichment_threshold_chars: usize,
    /// 单次 LLM 调用超时秒数（0=使用服务端默认）
    #[serde(default)]
    pub llm_timeout_secs: u64,
}

/// 对话消息（兼容 OpenAI / DeepSeek 格式）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// DeepSeek 思考链 / Claude thinking content（用于多轮工具调用上下文保持）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: String, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        }
    }

    pub fn assistant(content: String, reasoning: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: if content.is_empty() { None } else { Some(content) },
            reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            tool_call_id: None,
        }
    }
}

/// LLM 返回的工具调用
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

// ── 流事件（Tauri Event 推送给前端） ──

/// 流式 token（普通文本增量）
#[derive(Debug, Clone, Serialize)]
pub struct StreamToken {
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamDone;

/// Agent Loop 完成后返回的完整消息列表（含工具调用/结果），供前端持久化
#[derive(Debug, Clone, Serialize)]
pub struct StreamMessages {
    pub messages: Vec<ChatMessage>,
}

/// 模型配额耗尽事件：当 LLM 返回 429/402 或配额相关错误时发射
/// 前端据此标记模型为"已耗尽"并自动切换到备用模型
#[derive(Debug, Clone, Serialize)]
pub struct ModelQuotaExhausted {
    /// 当前请求的 API Base URL
    pub api_base: String,
    /// 当前请求的模型名
    pub model: String,
    /// 错误详情（截断至 300 字符）
    pub error_message: String,
}

/// LLM 调用失败重试通知：告诉前端清空之前的 token 缓冲，准备接收新一轮流式输出
#[derive(Debug, Clone, Serialize)]
pub struct StreamRetry {
    pub attempt: u32,
}

/// 最终答案开始：agent 多轮循环结束、进入面向用户的正式回答阶段
#[derive(Debug, Clone, Serialize)]
pub struct FinalAnswerStart;

/// 思考过程：开始（DeepSeek/Claude 的 reasoning_content 阶段启动）
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingStart;

/// 思考过程：增量文本片段
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingDelta {
    pub delta: String,
}

/// 上下文窗口压缩事件（agent loop 自动触发摘要压缩时发射）
#[derive(Debug, Clone, Serialize)]
pub struct ContextCompacted {
    /// 压缩前的估算 token 数
    pub before_tokens: u64,
    /// 压缩后的估算 token 数
    pub after_tokens: u64,
    /// 被压缩的消息范围摘要
    pub summary: String,
}

/// 工具执行审批请求（human-in-the-loop）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalRequired {
    pub tool_call_id: String,
    pub name: String,
    pub arguments: String,
}

/// 工具执行审批决策（前端通过 Tauri command 回传）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalDecision {
    pub tool_call_id: String,
    pub approved: bool,
    pub feedback: Option<String>,
}

impl Default for ToolApprovalDecision {
    fn default() -> Self {
        Self {
            tool_call_id: String::new(),
            approved: false,
            feedback: None,
        }
    }
}

/// Token 使用统计事件（每轮 LLM 调用后发射）
#[derive(Debug, Clone, Serialize)]
pub struct TokenUsageEvent {
    pub iteration: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_estimated: u64,
    pub context_limit: u64,
}

/// Agent Loop 轮次跟踪事件（每轮发射，供前端显示进度）
#[derive(Debug, Clone, Serialize)]
pub struct AgentIterationEvent {
    /// 当前轮次（从 1 开始）
    pub iteration: usize,
    /// 总轮次上限
    pub total: usize,
    /// 阶段：thinking | acting | observing | verifying
    pub phase: String,
    /// 本阶段已过毫秒数
    pub elapsed_ms: u64,
}

/// Agent Loop 整体统计事件（循环结束后发射）
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStats {
    /// 总迭代次数
    pub total_iterations: usize,
    /// 总耗时（毫秒）
    pub total_elapsed_ms: u64,
    /// 工具调用总次数
    pub total_tool_calls: usize,
    /// 上下文压缩触发的次数
    pub compaction_count: usize,
    /// 是否执行了验证（L2）
    pub verification_performed: bool,
}

/// 思考过程：结束（统计信息）
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingStop {
    /// 估算 token 数（约 1 token ≈ 4 字符，中英文混合）
    pub tokens: u64,
    /// 思考耗时（毫秒）
    pub duration_ms: u64,
}

/// 工具调用开始事件（前端可渲染「正在调用 xxx」）
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallEvent {
    pub name: String,
    pub arguments: String,
}

/// 工具调用结果事件
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultEvent {
    pub name: String,
    pub result: String,
    /// 是否执行成功
    #[serde(rename = "isError")]
    pub is_error: bool,
    /// 错误码（成功时为 None）
    pub error_code: Option<String>,
    /// 错误分类（如 TIMEOUT、TOOL_ERROR）
    pub error_category: Option<String>,
    /// 建议操作：retry | reconnect | none
    pub suggested_action: Option<String>,
}

// ── 内部工具函数 ──

/// 把内部 ChatMessage 转为 OpenAI 接口需要的 JSON（精确控制字段，避免 null 陷阱）
pub fn msg_to_value(m: &ChatMessage) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("role".to_string(), Value::String(m.role.clone()));
    match &m.content {
        Some(c) => map.insert("content".to_string(), Value::String(c.clone())),
        None => map.insert("content".to_string(), Value::Null),
    };
    // DeepSeek/Claude：工具调用场景需要回传 reasoning_content 以保持思考上下文
    if let Some(rc) = &m.reasoning_content {
        if !rc.is_empty() {
            map.insert("reasoning_content".to_string(), Value::String(rc.clone()));
        }
    }
    if let Some(tcs) = &m.tool_calls {
        let arr: Vec<Value> = tcs
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments }
                })
            })
            .collect();
        map.insert("tool_calls".to_string(), Value::Array(arr));
    }
    if let Some(tid) = &m.tool_call_id {
        map.insert("tool_call_id".to_string(), Value::String(tid.clone()));
    }
    Value::Object(map)
}

/// 累积流式工具调用（OpenAI 分片推送 tool_calls）
#[derive(Default)]
pub(crate) struct ToolCallAcc {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 从累积器中产出最终的 ToolCall 列表（忽略没有名字的无效项）
pub(crate) fn finalize_tool_calls(accs: Vec<ToolCallAcc>) -> Vec<ToolCall> {
    accs.into_iter()
        .filter(|a| !a.name.is_empty())
        .map(|a| ToolCall {
            id: a.id,
            name: a.name,
            arguments: a.arguments,
        })
        .collect()
}
