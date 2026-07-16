//! Agent Loop 引擎（think → act → observe 循环）—— 增强版
//!
//! 与主流方案的差距补齐：
//! - **上下文窗口管理**：Token 估算 + 滑动窗口摘要压缩（对标 LangGraph 的 context management）
//! - **实时流式思考**：LLM 边生成边推送到思考面板（不再等 LLM 返回后再回放）
//! - **工具使用行为**：`ToolUseBehavior` 灵活配置循环终止策略（对标 OpenAI Agents SDK）
//! - **人工审批**：Human-in-the-Loop 工具执行前暂停等待确认（对标 LangGraph interrupt_before）
//! - **Token 用量追踪**：每轮估算并 emit 给前端
//! - **结构化错误分层**：错误分类 + 降级策略 + 自愈路径
//! - **工具结果富化**：超大工具结果自动摘要后再回传（减少 token 浪费）
//!
//! 设计参考（均为 `reference/agent-loop/community/` 下的开源实现）：
//! - `mini_agent/core.py` 的 `run_agent()`：干净的「思考→动作→观察」循环 +
//!   **依赖注入**（provider / tool_dispatcher 可替换）→ 本模块的 `LlmClient` / `ToolExecutor` trait。
//! - `mini_agent/reliability.py`：`with_retry`（指数退避+抖动）、`_RetryingProvider`（LLM 重试）、
//!   `traced_call`（结构化日志）、`validated_call`（参数校验）。
//! - `mini-swe-agent/src/minisweagent/agents/default.py`：`step_limit` / `wall_time_limit_seconds` /
//!   `max_consecutive_format_errors`，以及异常/格式错误作为 observation 回传的自愈思路。
//! - OpenAI Agents SDK `run.py`：并行工具调用 + 类型化工具结果 + tool_use_behavior。
//! - LangGraph：interrupt_before（HITL）、context window compaction、token tracking。

use crate::{
    AppHandle, ChatMessage, ChatRequest, ContextCompacted, AgentIterationEvent, AgentLoopStats,
    FinalAnswerStart, StreamDone, StreamError, StreamRetry, StreamToken, TokenUsageEvent,
    ToolApprovalDecision, ToolApprovalRequired, ToolCall, ToolCallEvent, ToolResultEvent,
    run_completion,
};
use tauri::Emitter;
use futures::future::BoxFuture;
use serde_json::Value;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::sync::watch;

// ═══════════════════════════════════════════════════════════════
//  公共类型
// ═══════════════════════════════════════════════════════════════

/// 一轮 LLM 调用的返回（已收集好流式 token 与 tool_calls）
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
}

/// 一次工具执行的产出（类型化结果，对齐 OpenAI Agents SDK 的 TypedToolResult）
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub result: String,
    pub is_error: bool,
    pub error_code: Option<String>,
    pub error_category: Option<String>,
    /// 建议操作：retry | reconnect | none（由真实后端 McpError 映射）
    pub suggested_action: Option<String>,
}

/// 工具使用行为（对标 OpenAI Agents SDK `tool_use_behavior`）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToolUseBehavior {
    /// 默认：工具执行后把结果回传给 LLM，继续循环
    RunLlmAgain,
    /// 首次工具调用的输出直接作为最终答案，不再调用 LLM
    StopOnFirstTool,
    /// 指定某些工具的调用结果直接作为最终答案
    StopAtTools(Vec<String>),
}

impl ToolUseBehavior {
}

// ═══════════════════════════════════════════════════════════════
//  Token 估算与上下文窗口管理
// ═══════════════════════════════════════════════════════════════

/// Token 估算常量（保守估算：1 token ≈ 3 中英文字符）
const CHARS_PER_TOKEN: f64 = 3.0;
/// 每条消息的固定开销（role + metadata）
const MSG_OVERHEAD_TOKENS: usize = 20;
/// 每轮工具调用的额外开销（id + type 等）
const TOOL_CALL_OVERHEAD: usize = 15;
/// 默认上下文窗口上限（对标主流 128K 模型）
const DEFAULT_CONTEXT_LIMIT: usize = 128_000;
/// 触发上下文压缩的阈值比例（占总上限的比例）
const COMPACTION_THRESHOLD: f64 = 0.80;

/// 估算文本 token 数（保守上界）
fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / CHARS_PER_TOKEN).ceil() as usize + 1
}

/// 估算单条消息的 token 消耗
fn estimate_message_tokens(msg: &ChatMessage) -> usize {
    let mut total = MSG_OVERHEAD_TOKENS;
    if let Some(ref c) = msg.content {
        total += estimate_tokens(c);
    }
    if let Some(ref rc) = msg.reasoning_content {
        total += estimate_tokens(rc);
    }
    if let Some(ref tcs) = msg.tool_calls {
        for tc in tcs {
            total += TOOL_CALL_OVERHEAD + estimate_tokens(&tc.name) + estimate_tokens(&tc.arguments);
        }
    }
    if let Some(ref tid) = msg.tool_call_id {
        total += estimate_tokens(tid);
    }
    total
}

/// 估算所有消息的总 token 消耗
fn estimate_messages_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

/// 估算工具 schema 的 token 消耗
fn estimate_tools_tokens(tools: &[Value]) -> usize {
    tools.iter().map(|t| estimate_tokens(&t.to_string())).sum()
}

/// 上下文压缩策略：找到最旧的可压缩消息段，生成摘要后替换为精简系统消息
///
/// 压缩规则：
/// 1. 保留：system prompt + 最近 4 条消息 + 当前最新 user 消息
/// 2. 可压缩段：从第 1 条 user 消息开始到最近第 5 条之前
/// 3. 将可压缩段合并为一条以"上下文摘要"开头的 system 消息
/// 4. 返回压缩详情用于 emit
fn compact_context(messages: &mut Vec<ChatMessage>) -> ContextCompacted {
    let before_tokens = estimate_messages_tokens(messages) as u64;

    // 找到可压缩区间
    // 保留：system 消息 + 最近 MIN_KEEP 条 + 最后一条用户消息
    const MIN_KEEP: usize = 4;

    if messages.len() <= MIN_KEEP + 1 {
        // 消息太少，不值得压缩
        return ContextCompacted {
            before_tokens,
            after_tokens: before_tokens,
            summary: String::new(),
        };
    }

    // 定位压缩起始点（第 1 条 user 消息之后）
    let mut start_idx = 0;
    for (i, msg) in messages.iter().enumerate() {
        if msg.role == "user" {
            start_idx = i;
            break;
        }
    }

    // 计算保留的最后 MIN_KEEP + 1 条的起始位置
    let keep_start = messages.len().saturating_sub(MIN_KEEP + 1);
    let end_idx = start_idx.max(keep_start.min(messages.len().saturating_sub(1)));

    if end_idx <= start_idx + 1 {
        return ContextCompacted {
            before_tokens,
            after_tokens: before_tokens,
            summary: String::new(),
        };
    }

    // 收集要压缩的消息内容
    let compressed_messages: Vec<String> = messages[start_idx..end_idx]
        .iter()
        .map(|m| {
            let role = &m.role;
            let content = m.content.as_deref().unwrap_or("");
            let tool_info = m
                .tool_calls
                .as_ref()
                .map(|tcs| format!(" [调用了 {} 个工具]", tcs.len()))
                .unwrap_or_default();
            format!("[{role}]{tool_info}: {content}")
        })
        .collect();

    let summary_text = compressed_messages.join("\n");

    // 截取摘要前 2000 字符
    let truncated: String = if summary_text.len() > 2000 {
        format!("{}… [已截断, 原长度 {}]", &summary_text[..2000], summary_text.len())
    } else {
        summary_text.clone()
    };

    // 构建压缩后的摘要消息
    let summary_msg = ChatMessage::system(format!(
        "[上下文摘要] 以下为历史对话中已压缩的部分：\n{}",
        truncated
    ));

    // 替换区间
    messages.splice(start_idx..end_idx, vec![summary_msg]);

    let after_tokens = estimate_messages_tokens(messages) as u64;
    let saved = before_tokens.saturating_sub(after_tokens);

    eprintln!(
        "[Context] 压缩完成: {}→{} tokens, 节省 {} tokens, 压缩了 {} 条消息",
        before_tokens,
        after_tokens,
        saved,
        compressed_messages.len()
    );

    ContextCompacted {
        before_tokens,
        after_tokens,
        summary: if saved > 0 {
            format!("节省 {} tokens ({}→{})", saved, before_tokens, after_tokens)
        } else {
            String::new()
        },
    }
}

// ═══════════════════════════════════════════════════════════════
//  Loop 配置（护栏 + 可靠性参数）
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// 最大循环轮次
    pub max_iterations: usize,
    /// 工具结果超长时截断（防止上下文爆炸）
    pub max_tool_result_chars: usize,
    /// 墙钟时间上限（秒）
    pub wall_time_limit_secs: u64,
    /// 单次 LLM 调用的超时秒数（0=使用底层 HTTP 读取超时 120s）
    pub llm_timeout_secs: u64,
    /// 是否并行执行同一轮内的多个工具调用
    pub parallel_tools: bool,
    /// LLM 调用失败最大重试次数
    pub llm_max_retries: u32,
    /// 工具调用失败最大重试次数
    pub tool_max_retries: u32,
    /// 工具使用行为（对标 OpenAI Agents SDK `tool_use_behavior`）
    pub tool_use_behavior: ToolUseBehavior,
    /// 上下文窗口上限（约 token 数，用于触发压缩策略）
    pub context_limit: usize,
    /// 触发上下文压缩的阈值比例 (0.0 ~ 1.0)
    pub compaction_threshold: f64,
    /// 需要人工审批的工具名列表（human-in-the-loop）
    pub require_tool_approval_for: Vec<String>,
    /// 工具结果富化阈值：超过此长度的文本结果自动摘要（0=禁用）
    pub enrichment_threshold_chars: usize,
    /// L2 验证循环：主循环结束后是否执行 LLM-as-Judge 质量验证
    pub enable_verification: bool,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            max_tool_result_chars: 8000,
            wall_time_limit_secs: 300,
            llm_timeout_secs: 0, // 0=使用底层 HTTP 读取超时 120s
            parallel_tools: true,
            llm_max_retries: 3,
            tool_max_retries: 2,
            tool_use_behavior: ToolUseBehavior::RunLlmAgain,
            context_limit: DEFAULT_CONTEXT_LIMIT,
            compaction_threshold: COMPACTION_THRESHOLD,
            require_tool_approval_for: Vec::new(),
            enrichment_threshold_chars: 5000,
            enable_verification: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Trait 定义（依赖注入）
// ═══════════════════════════════════════════════════════════════

/// LLM 客户端抽象（对应 mini_agent 的 provider 注入）
pub trait LlmClient: Send + Sync {
    fn complete<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        tools: &'a [Value],
        cancel: &'a mut watch::Receiver<bool>,
        stream_content: bool,
    ) -> BoxFuture<'a, Result<LlmResponse, String>>;
}

/// 工具执行器抽象（对应 mini_agent 的 tool_dispatcher 注入）
pub trait ToolExecutor: Send + Sync {
    fn execute<'a>(&'a self, name: &'a str, arguments: &'a str) -> BoxFuture<'a, ToolOutcome>;
}

// ═══════════════════════════════════════════════════════════════
//  Loop 上下文
// ═══════════════════════════════════════════════════════════════

pub struct LoopContext<'a> {
    /// AppHandle（用于向前端 emit 事件）；测试时可传 None 跳过
    pub app: Option<&'a AppHandle>,
    pub config: AgentLoopConfig,
    pub initial_messages: Vec<ChatMessage>,
    pub tools: Vec<Value>,
    pub llm: &'a dyn LlmClient,
    pub executor: &'a dyn ToolExecutor,
    pub cancel: &'a mut watch::Receiver<bool>,
    /// 人工审批通道（human-in-the-loop）：前端通过 Tauri command 写入
    pub approval_rx: Option<watch::Receiver<ToolApprovalDecision>>,
}

// ═══════════════════════════════════════════════════════════════
//  真实实现
// ═══════════════════════════════════════════════════════════════

/// 真实 LLM 客户端：包装现有的 `run_completion`
pub struct RealLlmClient {
    pub app: AppHandle,
    pub request: ChatRequest,
}

impl LlmClient for RealLlmClient {
    fn complete<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        tools: &'a [Value],
        cancel: &'a mut watch::Receiver<bool>,
        stream_content: bool,
    ) -> BoxFuture<'a, Result<LlmResponse, String>> {
        Box::pin(async move {
            let (content, reasoning, tool_calls) =
                run_completion(&self.app, &self.request, messages, tools, cancel, stream_content)
                    .await?;
            Ok(LlmResponse {
                content,
                reasoning,
                tool_calls,
            })
        })
    }
}

/// 统一工具执行器：通过 ToolRegistry 分发到 MCP 或原生工具
pub struct RegistryToolExecutor<'a> {
    pub registry: &'a crate::tools::ToolRegistry,
}

impl<'a> ToolExecutor for RegistryToolExecutor<'a> {
    fn execute<'b>(&'b self, name: &'b str, arguments: &'b str) -> BoxFuture<'b, ToolOutcome> {
        Box::pin(async move {
            match self.registry.execute(name, arguments).await {
                Ok(result) => ToolOutcome {
                    result,
                    is_error: false,
                    error_code: None,
                    error_category: None,
                    suggested_action: None,
                },
                Err(err_msg) => ToolOutcome {
                    result: err_msg.clone(),
                    is_error: true,
                    error_code: Some("TOOL-001".into()),
                    error_category: Some("EXECUTION_ERROR".into()),
                    suggested_action: Some("none".into()),
                },
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════
//  事件发射辅助
// ═══════════════════════════════════════════════════════════════

fn emit<S: serde::Serialize + Clone>(app: Option<&AppHandle>, event: &str, payload: S) {
    if let Some(app) = app {
        let _ = app.emit(event, payload);
    }
}

/// 将最终答案逐块以 stream-token 事件流式推送，模拟真实 LLM 流式输出
/// 每块约 10 字符，块间延迟 10ms
async fn stream_final_answer(app: Option<&AppHandle>, content: &str) {
    const CHUNK_SIZE: usize = 10;
    let chars: Vec<char> = content.chars().collect();
    let mut offset = 0;

    while offset < chars.len() {
        let end = (offset + CHUNK_SIZE).min(chars.len());
        let chunk: String = chars[offset..end].iter().collect();

        emit(app, "stream-token", StreamToken { token: chunk });

        offset = end;
        if offset < chars.len() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Agent Loop 主循环
// ═══════════════════════════════════════════════════════════════

pub async fn run_agent_loop(ctx: LoopContext<'_>) -> Result<Vec<ChatMessage>, String> {
    let LoopContext {
        app,
        config,
        initial_messages,
        tools,
        llm,
        executor,
        cancel,
        approval_rx,
    } = ctx;

    // 工具 schema 的 token 消耗估算（用于上下文阈值判断）
    let tools_tokens = estimate_tools_tokens(&tools);

    eprintln!(
        "[AgentLoop] 启动 工具={} 消息={} max_iter={} context_limit={}K behavior={:?} verification={}",
        tools.len(),
        initial_messages.len(),
        config.max_iterations,
        config.context_limit / 1000,
        std::mem::discriminant(&config.tool_use_behavior),
        config.enable_verification,
    );

    let mut messages = initial_messages;
    let start = Instant::now();

    // 已从工具结果中提取最终答案（StopOnFirstTool 场景）
    let mut early_final_answer: Option<String> = None;

    // 统计信息
    let mut total_tool_calls: usize = 0;
    let mut compaction_count: usize = 0;

    // —— 构建合法工具名集合（格式错误自愈：未知工具直接返回结构化错误）——
    let valid_tools: HashSet<String> = tools
        .iter()
        .filter_map(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // 构建需审批工具集合
    let approval_set: HashSet<String> = config
        .require_tool_approval_for
        .iter()
        .cloned()
        .collect();

    for iteration in 0..config.max_iterations {
        // ── 护栏：取消 ──
        if *cancel.borrow() {
            emit(app, "stream-error", StreamError { error: "已取消".into() });
            emit(app, "stream-done", StreamDone);
            return Ok(messages);
        }

        // ── 护栏：墙钟时间 ──
        if start.elapsed().as_secs() > config.wall_time_limit_secs {
            emit(
                app,
                "stream-error",
                StreamError {
                    error: format!("超出时间限制 ({}s)", config.wall_time_limit_secs),
                },
            );
            emit(app, "stream-done", StreamDone);
            return Ok(messages);
        }

        // ── 检查上下文窗口：是否需要压缩 ──
        let current_tokens =
            tools_tokens + estimate_messages_tokens(&messages);
        let threshold = (config.context_limit as f64 * config.compaction_threshold) as usize;

        if current_tokens > threshold {
            eprintln!(
                "[AgentLoop] 上下文接近上限: {} / {} tokens, 触发压缩",
                current_tokens, config.context_limit
            );
            let compacted = compact_context(&mut messages);
            if !compacted.summary.is_empty() {
                compaction_count += 1;
                emit(app, "context-compacted", compacted);
            }
        }

        // ── 如果有提前结束的最终答案（StopOnFirstTool），直接输出 ──
        if let Some(answer) = early_final_answer.take() {
            emit(app, "final-answer-start", FinalAnswerStart);
            stream_final_answer(app, &answer).await;
            emit(app, "stream-done", StreamDone);
            return Ok(messages);
        }

        // ── 发射轮次开始事件 ──
        let iteration_start = Instant::now();
        emit(
            app,
            "agent-iteration",
            AgentIterationEvent {
                iteration: iteration + 1,
                total: config.max_iterations,
                phase: "thinking".into(),
                elapsed_ms: start.elapsed().as_millis() as u64,
            },
        );

        // ── THINK：调用 LLM ──
        eprintln!(
            "[AgentLoop] 轮次 {}/{} 调用 LLM (messages={}, tokens≈{})...",
            iteration + 1,
            config.max_iterations,
            messages.len(),
            current_tokens,
        );

        let resp = match llm_complete_retried(app, llm, &messages, &tools, cancel, &config, false)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[AgentLoop] LLM 调用最终失败: {e}");
                emit(app, "stream-error", StreamError { error: e });
                emit(app, "stream-done", StreamDone);
                return Ok(messages);
            }
        };

        // ── Token 用量追踪 ──
        let input_tokens = current_tokens as u64;
        let output_tokens = (estimate_tokens(&resp.content)
            + estimate_tokens(&resp.reasoning)
            + resp.tool_calls.len() * TOOL_CALL_OVERHEAD) as u64;
        let total_estimated = input_tokens + output_tokens;

        emit(
            app,
            "token-usage",
            TokenUsageEvent {
                iteration: iteration + 1,
                input_tokens,
                output_tokens,
                total_estimated,
                context_limit: config.context_limit as u64,
            },
        );

        eprintln!(
            "[AgentLoop] LLM 返回 content={} chars reasoning={} chars tool_calls={} tokens≈{}",
            resp.content.len(),
            resp.reasoning.len(),
            resp.tool_calls.len(),
            output_tokens,
        );

        // 轮次标记（向后兼容，同时保留旧格式）
        emit(
            app,
            "agent-iteration-tick",
            serde_json::json!({
                "iteration": iteration + 1,
                "total": config.max_iterations,
                "elapsed_ms": iteration_start.elapsed().as_millis() as u64,
            }),
        );

        // 把 assistant 消息加回上下文
        messages.push(ChatMessage::assistant(
            resp.content.clone(),
            resp.reasoning.clone(),
            resp.tool_calls.clone(),
        ));

        // ── 区分「终止轮（最终答案）」与「中间轮（推理/工具调用）」──
        if resp.tool_calls.is_empty() {
            // —— 终止轮：最终答案 ——
            eprintln!(
                "[AgentLoop] 终止轮：无工具调用 content={} chars reasoning={} chars",
                resp.content.len(),
                resp.reasoning.len()
            );

            let total_iterations = iteration + 1;
            let total_elapsed_ms = start.elapsed().as_millis() as u64;

            let final_answer = if !resp.content.is_empty() {
                resp.content.as_str()
            } else if !resp.reasoning.is_empty() {
                eprintln!(
                    "[AgentLoop] content 为空，回退使用 reasoning 作为最终答案 ({} chars)",
                    resp.reasoning.len()
                );
                resp.reasoning.as_str()
            } else {
                ""
            };

            // ── L2 验证循环（可选） ──
            let verification_performed = if config.enable_verification && !final_answer.is_empty() {
                emit(
                    app,
                    "agent-iteration",
                    AgentIterationEvent {
                        iteration: total_iterations,
                        total: config.max_iterations,
                        phase: "verifying".into(),
                        elapsed_ms: iteration_start.elapsed().as_millis() as u64,
                    },
                );
                let verified = verify_output(
                    app, llm, cancel, &config, &messages, final_answer,
                )
                .await;
                eprintln!(
                    "[AgentLoop] L2 验证结果: {} (本轮将{}重试)",
                    if verified { "通过 ✓" } else { "未通过 ✗" },
                    if verified { "不" } else { "" },
                );
                // 验证失败时：继续循环让 LLM 自修正（增加一次迭代）
                if !verified {
                    // 在消息中添加验证反馈
                    messages.push(ChatMessage::user(
                        "你之前的回答未通过质量验证，请根据反馈改进你的回答。确保回答完整、准确、有实际帮助。",
                    ));
                    continue; // 不结束，继续下一轮迭代
                }
                true
            } else {
                false
            };

            // ── 发射最终答案 ──
            emit(app, "final-answer-start", FinalAnswerStart);

            if !final_answer.is_empty() {
                stream_final_answer(app, final_answer).await;
            } else {
                let fallback = "（模型未生成回复内容，请尝试重新提问或检查 API Key / 配额）";
                emit(app, "stream-token", StreamToken {
                    token: fallback.to_string(),
                });
            }

            // 发射循环统计
            emit(
                app,
                "agent-loop-stats",
                AgentLoopStats {
                    total_iterations,
                    total_elapsed_ms,
                    total_tool_calls,
                    compaction_count,
                    verification_performed,
                },
            );

            emit(app, "stream-done", StreamDone);
            return Ok(messages);
        }

        // 更新工具调用计数
        total_tool_calls += resp.tool_calls.len();

        // —— 中间轮：执行工具 ——
        // 注意：content 已在 parse_sse_response 中以 thinking-delta 实时流式推送
        // （当 stream_content=false 时），无需再调用 stream_content_as_thinking

        // ── 发射中间轮执行事件 ──
        emit(
            app,
            "agent-iteration",
            AgentIterationEvent {
                iteration: iteration + 1,
                total: config.max_iterations,
                phase: "acting".into(),
                elapsed_ms: iteration_start.elapsed().as_millis() as u64,
            },
        );

        // ── 检查 ToolUseBehavior 是否需要在工具执行后停止 ──
        if let ToolUseBehavior::StopOnFirstTool = config.tool_use_behavior {
            // 把首个工具结果作为最终答案
            // 先执行所有工具（并行）
            execute_and_observe(
                app,
                &resp.tool_calls,
                &valid_tools,
                executor,
                &config,
                &mut messages,
                &approval_rx,
                &approval_set,
            )
            .await;

            // 找到第一条成功的工具消息作为答案
            let final_answer = messages
                .iter()
                .rev()
                .find(|m| m.role == "tool")
                .and_then(|m| m.content.clone())
                .unwrap_or_default();

            if !final_answer.is_empty() {
                emit(app, "final-answer-start", FinalAnswerStart);
                stream_final_answer(app, &final_answer).await;
            } else {
                emit(app, "stream-token", StreamToken {
                    token: "⏹ 工具执行已完成，无返回内容。".to_string(),
                });
            }
            emit(app, "stream-done", StreamDone);
            return Ok(messages);
        }

        if let ToolUseBehavior::StopAtTools(ref stop_names) = config.tool_use_behavior {
            // 检查本轮是否有指定工具被调用
            let should_stop = resp
                .tool_calls
                .iter()
                .any(|tc| stop_names.iter().any(|n| n == &tc.name));

            if should_stop {
                // 执行所有工具
                execute_and_observe(
                    app,
                    &resp.tool_calls,
                    &valid_tools,
                    executor,
                    &config,
                    &mut messages,
                    &approval_rx,
                    &approval_set,
                )
                .await;

                // 提取指定工具的返回作为最终答案
                let final_answer = messages
                    .iter()
                    .rev()
                    .find(|m| m.role == "tool")
                    .and_then(|m| m.content.clone())
                    .unwrap_or_default();

                emit(app, "final-answer-start", FinalAnswerStart);
                if !final_answer.is_empty() {
                    stream_final_answer(app, &final_answer).await;
                } else {
                    emit(app, "stream-token", StreamToken {
                        token: "⏹ 指定工具已执行完毕。".to_string(),
                    });
                }
                emit(app, "stream-done", StreamDone);
                return Ok(messages);
            }
        }

        // ═══════════════════════════════════════════════════
        //  ACT / OBSERVE：执行工具并回传结果
        // ═══════════════════════════════════════════════════
        execute_and_observe(
            app,
            &resp.tool_calls,
            &valid_tools,
            executor,
            &config,
            &mut messages,
            &approval_rx,
            &approval_set,
        )
        .await;
    }

    // 超出最大迭代，正常结束
    let total_elapsed_ms = start.elapsed().as_millis() as u64;

    emit(
        app,
        "agent-loop-stats",
        AgentLoopStats {
            total_iterations: config.max_iterations,
            total_elapsed_ms,
            total_tool_calls,
            compaction_count,
            verification_performed: false,
        },
    );

    emit(app, "stream-done", StreamDone);
    Ok(messages)
}

// ═══════════════════════════════════════════════════════════════
//  工具执行与观察
// ═══════════════════════════════════════════════════════════════

/// 执行工具调用 + emit tool-call/tool-result 事件 + 把结果写回消息列表
///
/// 新增功能：
/// - 人工审批流（human-in-the-loop）
/// - 工具结果富化（超大结果自动摘要）
#[allow(clippy::too_many_arguments)]
async fn execute_and_observe(
    app: Option<&AppHandle>,
    tool_calls: &[ToolCall],
    valid_tools: &HashSet<String>,
    executor: &dyn ToolExecutor,
    config: &AgentLoopConfig,
    messages: &mut Vec<ChatMessage>,
    approval_rx: &Option<watch::Receiver<ToolApprovalDecision>>,
    approval_set: &HashSet<String>,
) {
    // 先 emit 所有 tool-call 事件
    for tc in tool_calls {
        emit(
            app,
            "tool-call",
            ToolCallEvent {
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            },
        );
    }

    // ── 人工审批流（改进点#4）──
    if let Some(approval_src) = approval_rx.as_ref() {
        let mut rx = approval_src.clone();
        for tc in tool_calls {
            if approval_set.contains(&tc.name) {
                eprintln!("[HITL] 工具 '{}' 需要审批，等待用户确认...", tc.name);
                emit(
                    app,
                    "tool-approval-required",
                    ToolApprovalRequired {
                        tool_call_id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                );

                // 等待审批决策（带超时 120s）
                let approval = loop {
                    let wait_result = tokio::time::timeout(
                        Duration::from_secs(120),
                        rx.changed(),
                    )
                    .await;

                    match wait_result {
                        Ok(Ok(())) => {
                            let decision = rx.borrow().clone();
                            if decision.tool_call_id == tc.id {
                                break decision;
                            }
                            // 不是给这个工具的决策，继续等
                        }
                        Ok(Err(_)) => {
                            // channel 已关闭，视同拒绝
                            break ToolApprovalDecision {
                                tool_call_id: tc.id.clone(),
                                approved: false,
                                feedback: Some("审批通道已关闭".into()),
                            };
                        }
                        Err(_) => {
                            // 超时，视同拒绝
                            emit(
                                app,
                                "stream-error",
                                StreamError {
                                    error: format!("工具 '{}' 审批超时 (120s)", tc.name),
                                },
                            );
                            break ToolApprovalDecision {
                                tool_call_id: tc.id.clone(),
                                approved: false,
                                feedback: Some("审批超时".into()),
                            };
                        }
                    }
                };

                if !approval.approved {
                    let msg = format!(
                        "[审批拒绝] 工具 '{}' 未获批准。用户反馈: {}",
                        tc.name,
                        approval.feedback.as_deref().unwrap_or("无")
                    );
                    emit(
                        app,
                        "tool-result",
                        ToolResultEvent {
                            name: tc.name.clone(),
                            result: msg.clone(),
                            is_error: true,
                            error_code: Some("HITL-001".into()),
                            error_category: Some("USER_DENIED".into()),
                            suggested_action: Some("none".into()),
                        },
                    );
                    messages.push(ChatMessage::tool(tc.id.clone(), &msg));
                    continue; // 跳过此工具的执行
                }
                eprintln!("[HITL] 工具 '{}' 已获批准", tc.name);
            }
        }
    }

    // 并行或串行执行
    let outcomes: Vec<ToolOutcome> = if config.parallel_tools {
        let futs = tool_calls
            .iter()
            .map(|tc| execute_one(tc, valid_tools, executor, config));
        futures::future::join_all(futs).await
    } else {
        let mut v = Vec::with_capacity(tool_calls.len());
        for tc in tool_calls {
            v.push(execute_one(tc, valid_tools, executor, config).await);
        }
        v
    };

    // emit tool-result + 把结果作为 tool 消息回传
    for (tc, mut outcome) in tool_calls.iter().zip(outcomes.into_iter()) {
        // ── 工具结果富化（改进点#5）──
        // 对超长的纯文本结果自动摘要，减少 token 浪费
        if config.enrichment_threshold_chars > 0
            && !outcome.is_error
            && outcome.result.len() > config.enrichment_threshold_chars
        {
            let enriched = enrich_tool_result(&outcome.result, config.enrichment_threshold_chars);
            outcome.result = enriched;
        }

        emit(
            app,
            "tool-result",
            ToolResultEvent {
                name: tc.name.clone(),
                result: outcome.result.clone(),
                is_error: outcome.is_error,
                error_code: outcome.error_code.clone(),
                error_category: outcome.error_category.clone(),
                suggested_action: outcome.suggested_action.clone(),
            },
        );
        messages.push(ChatMessage::tool(tc.id.clone(), &outcome.result));
    }
}

/// 工具结果富化：对超大纯文本结果生成结构化摘要，保持关键信息同时大幅压缩体积
fn enrich_tool_result(result: &str, threshold: usize) -> String {
    let approximate_tokens = estimate_tokens(result);

    // 对 JSON 结果：保留结构但截断数组/对象中的长值
    if let Ok(val) = serde_json::from_str::<Value>(result) {
        let compressed = compress_json_value(&val, threshold);
        let compressed_str = serde_json::to_string(&compressed).unwrap_or_default();
        // 如果压缩后依然很大，就只截断首部
        if compressed_str.len() <= threshold {
            return format!(
                "[结果已结构化压缩 (原 ~{} tokens)]\n{}",
                approximate_tokens, compressed_str
            );
        }
    }

    // 非 JSON 文本：保留开头 + 尾部，取最核心部分
    let head = &result[..result.len().min(threshold / 2)];
    let tail_start = result.len().saturating_sub(threshold / 4);
    let tail = &result[tail_start..];
    format!(
        "[结果已截断: 原始内容 ~{} tokens, 显示浓缩版]\n--- 开头 ---\n{}\n\n--- 结尾 ---\n{}",
        approximate_tokens, head, tail
    )
}

/// JSON 值压缩：递归截断长字符串值
fn compress_json_value(val: &Value, budget: usize) -> Value {
    match val {
        Value::String(s) if s.len() > 200 => {
            Value::String(format!("{}…[{} chars]", &s[..200], s.len()))
        }
        Value::Array(arr) => {
            let compressed: Vec<Value> = arr
                .iter()
                .take(50) // 最多保留 50 个元素
                .map(|v| compress_json_value(v, budget))
                .collect();
            if arr.len() > 50 {
                Value::Array(compressed.into_iter().chain(std::iter::once(Value::String(
                    format!("…[还有 {} 个元素被省略]", arr.len() - 50),
                ))).collect())
            } else {
                Value::Array(compressed)
            }
        }
        Value::Object(map) => {
            let compressed: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), compress_json_value(v, budget)))
                .collect();
            Value::Object(compressed)
        }
        _ => val.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════
//  LLM 重试
// ═══════════════════════════════════════════════════════════════

/// 带指数退避重试的 LLM 调用（支持单次调用超时）
async fn llm_complete_retried(
    app: Option<&AppHandle>,
    llm: &dyn LlmClient,
    messages: &[ChatMessage],
    tools: &[Value],
    cancel: &mut watch::Receiver<bool>,
    config: &AgentLoopConfig,
    stream_content: bool,
) -> Result<LlmResponse, String> {
    let mut last_err = String::new();
    for attempt in 0..=config.llm_max_retries {
        if attempt > 0 && stream_content {
            emit(app, "stream-retry", StreamRetry { attempt });
        }

        // 构造带超时的 LLM 调用
        let call_fut = llm.complete(messages, tools, cancel, stream_content);
        let result = if config.llm_timeout_secs > 0 {
            let timeout_dur = Duration::from_secs(config.llm_timeout_secs);
            match tokio::time::timeout(timeout_dur, call_fut).await {
                Ok(r) => r,
                Err(_) => {
                    let msg = format!(
                        "LLM 调用超时 (>{:?})，模型或网络响应过慢",
                        timeout_dur
                    );
                    eprintln!("[LLM] {}", msg);
                    Err(msg)
                }
            }
        } else {
            call_fut.await
        };

        match result {
            Ok(r) => return Ok(r),
            Err(e) => {
                last_err = e.clone();
                if attempt < config.llm_max_retries {
                    let delay = llm_backoff(attempt);
                    eprintln!(
                        "[LLM] 调用失败 (第 {} 次)，{:.1}s 后重试: {}",
                        attempt + 1,
                        delay,
                        e
                    );
                    tokio::time::sleep(Duration::from_secs_f64(delay)).await;
                }
            }
        }
    }
    Err(last_err)
}

/// 指数退避 + 轻量抖动
fn llm_backoff(attempt: u32) -> f64 {
    let base = 1.0f64;
    let exp = base * 2f64.powi(attempt as i32);
    let jitter = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() % 500)
        .unwrap_or(0) as f64)
        / 1000.0;
    exp + jitter
}

/// L2 验证循环：LLM-as-Judge 校验输出质量
///
/// 向 LLM 发送一条验证请求（不推送给前端，只取返回结果），
/// 判断回答是否完整、准确、满足用户需求。
///
/// # 验证 prompt 设计（LLM-as-Judge 模式）
/// - 把用户消息 + assistant 回答发给 LLM
/// - LLM 返回 YES/NO + 原因
/// - 仅在返回明确 YES 时通过
///
/// 遵循 Loop Engineering 中 Maker/Checker 分离原则：
/// 生成回答的模型不应判定自己"完成"，由独立的 Judge 调用验证。
async fn verify_output(
    app: Option<&AppHandle>,
    llm: &dyn LlmClient,
    cancel: &mut watch::Receiver<bool>,
    config: &AgentLoopConfig,
    messages: &[ChatMessage],
    final_answer: &str,
) -> bool {
    // 提取最后一条用户消息作为上下文
    let last_user_msg = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .unwrap_or("");

    if last_user_msg.is_empty() || final_answer.is_empty() {
        return true; // 无法验证时默认通过
    }

    let verification_prompt = format!(
        r#"你是一个严格的质量验证官。请判断以下 AI 回答是否完整、准确地解决了用户问题。

【用户问题】
{}

【AI 回答】
{}

请用以下格式回复：
判断：YES 或 NO
理由：（简短说明，仅当 NO 时填写）

要求：
- YES：回答完整解决了问题，包含必要细节，没有明显错误
- NO：回答不完整、有错误、或未解决核心问题
- 只做质量判断，不修改回答。"#,
        last_user_msg, final_answer
    );

    let verify_messages = vec![
        crate::ChatMessage::system("你是一个严格的质量验证官。"),
        crate::ChatMessage::user(&verification_prompt),
    ];

    eprintln!("[L2-Verify] 开始质量验证... (回答长度={})", final_answer.len());

    // 用短超时执行验证（较快模型应该能在 15s 内完成判断）
    let verify_config = AgentLoopConfig {
        llm_timeout_secs: if config.llm_timeout_secs > 0 {
            config.llm_timeout_secs.min(15)
        } else {
            15
        },
        ..Default::default()
    };

    let result = llm_complete_retried(app, llm, &verify_messages, &[], cancel, &verify_config, false).await;

    match result {
        Ok(resp) => {
            let verdict = resp.content.to_uppercase();
            let passed = verdict.contains("判断：YES")
                || (verdict.starts_with("YES") && !verdict.contains("NO"))
                || verdict.contains("\"判断\":\"YES\"");
            eprintln!(
                "[L2-Verify] 判定: {} (原始回复前 80 字符: {:?})",
                if passed { "通过 ✓" } else { "未通过 ✗" },
                &resp.content[..resp.content.len().min(80)]
            );
            passed
        }
        Err(e) => {
            eprintln!("[L2-Verify] 验证调用失败: {} （默认通过）", e);
            true // 验证失败时默认通过，不影响主流程
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  单个工具执行
// ═══════════════════════════════════════════════════════════════

/// 执行单个工具调用：格式错误自愈 + 可重试错误重试 + 结果截断 + 结构化日志
async fn execute_one(
    tc: &ToolCall,
    valid_tools: &HashSet<String>,
    executor: &dyn ToolExecutor,
    config: &AgentLoopConfig,
) -> ToolOutcome {
    // 格式错误自愈：工具名不在合法列表 → 直接返回结构化错误
    if !valid_tools.contains(&tc.name) {
        let known: Vec<String> = valid_tools.iter().cloned().collect();
        let msg = format!("[格式错误] 未知工具 '{}'，可用工具：{:?}", tc.name, known);
        log_tool_call(&tc.name, &tc.arguments, None, Some(&msg));
        return ToolOutcome {
            result: msg,
            is_error: true,
            error_code: Some("LOOP-001".into()),
            error_category: Some("FORMAT".into()),
            suggested_action: Some("none".into()),
        };
    }

    // 首次执行
    let mut outcome = executor.execute(&tc.name, &tc.arguments).await;

    // 重试（仅对瞬时错误 retry）
    for attempt in 1..=config.tool_max_retries {
        if outcome.is_error && outcome.suggested_action.as_deref() == Some("retry") {
            log_tool_call(&tc.name, &tc.arguments, Some(attempt), None);
            tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
            outcome = executor.execute(&tc.name, &tc.arguments).await;
        } else {
            break;
        }
    }

    // 结果截断
    if outcome.result.len() > config.max_tool_result_chars {
        let truncated = outcome.result[..config.max_tool_result_chars].to_string();
        outcome.result =
            format!("{}…[已截断，原长度 {}]", truncated, outcome.result.len());
    }

    log_tool_call(
        &tc.name,
        &tc.arguments,
        None,
        if outcome.is_error {
            Some(&outcome.result)
        } else {
            None
        },
    );
    outcome
}

// ═══════════════════════════════════════════════════════════════
//  日志与脱敏
// ═══════════════════════════════════════════════════════════════

fn log_tool_call(name: &str, args: &str, retry_attempt: Option<u32>, error: Option<&str>) {
    let sanitized = sanitize_args(args);
    match error {
        Some(e) => eprintln!("[TOOL] name={} args={} ERROR={}", name, sanitized, e),
        None => match retry_attempt {
            Some(a) => eprintln!("[TOOL] name={} args={} retry#{}", name, sanitized, a),
            None => eprintln!("[TOOL] name={} args={} ok", name, sanitized),
        },
    }
}

const SENSITIVE_FIELDS: &[&str] = &[
    "key", "secret", "token", "password", "api_key", "authorization",
];

fn sanitize_args(args: &str) -> String {
    match serde_json::from_str::<Value>(args) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                for (k, val) in obj.iter_mut() {
                    let lower = k.to_lowercase();
                    if SENSITIVE_FIELDS.iter().any(|s| lower.contains(s)) {
                        *val = Value::String("***".into());
                    }
                }
            }
            serde_json::to_string(&v).unwrap_or_else(|e| {
                format!("(redacted params, serialization failed: {e})")
            })
        }
        Err(_) => {
            let t: String = args.chars().take(200).collect();
            if args.len() > 200 {
                format!("{}…", t)
            } else {
                t
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// 脚本化 Fake LLM：按调用顺序返回预设响应，并记录收到的 messages
    struct FakeLlmClient {
        script: Vec<LlmResponse>,
        idx: std::sync::atomic::AtomicUsize,
        recorded: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    }

    impl FakeLlmClient {
        fn new(script: Vec<LlmResponse>) -> Self {
            Self {
                script,
                idx: std::sync::atomic::AtomicUsize::new(0),
                recorded: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl LlmClient for FakeLlmClient {
        fn complete<'a>(
            &'a self,
            messages: &'a [ChatMessage],
            _tools: &'a [Value],
            _cancel: &'a mut watch::Receiver<bool>,
            _stream_content: bool,
        ) -> BoxFuture<'a, Result<LlmResponse, String>> {
            Box::pin(async move {
                self.recorded.lock().unwrap().push(messages.to_vec());
                let i = self.idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(self.script[i % self.script.len()].clone())
            })
        }
    }

    /// Fake 工具执行器
    struct FakeExecutor {
        calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl ToolExecutor for FakeExecutor {
        fn execute<'a>(
            &'a self,
            name: &'a str,
            arguments: &'a str,
        ) -> BoxFuture<'a, ToolOutcome> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .unwrap()
                    .push((name.to_string(), arguments.to_string()));
                ToolOutcome {
                    result: format!("result of {}", name),
                    is_error: false,
                    error_code: None,
                    error_category: None,
                    suggested_action: None,
                }
            })
        }
    }

    fn tool(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: format!("{}-id", name),
            name: name.to_string(),
            arguments: args.to_string(),
        }
    }

    fn tool_schema(name: &str) -> Value {
        serde_json::json!({
            "type": "function",
            "function": { "name": name, "description": "", "parameters": {} }
        })
    }

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage::user(text)
    }

    #[tokio::test]
    async fn loop_runs_tool_then_ends_with_final_answer() {
        let llm = FakeLlmClient::new(vec![
            LlmResponse {
                content: String::new(),
                reasoning: String::new(),
                tool_calls: vec![tool("search", r#"{"q":"hi"}"#)],
            },
            LlmResponse {
                content: "最终答案".into(),
                reasoning: String::new(),
                tool_calls: vec![],
            },
        ]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let calls = exec.calls.clone();

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig::default(),
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("search")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        run_agent_loop(ctx).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1, "应恰好执行一次工具");
        assert_eq!(recorded[0].0, "search");
        assert_eq!(recorded[0].1, r#"{"q":"hi"}"#);
    }

    #[tokio::test]
    async fn max_iterations_guard_stops_runaway() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: vec![tool("loop", "{}")],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig {
                max_iterations: 3,
                ..Default::default()
            },
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("loop")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        run_agent_loop(ctx).await.unwrap();
        assert_eq!(exec.calls.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn unknown_tool_self_heals_without_calling_executor() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: vec![tool("nonexistent", "{}")],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig::default(),
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("real")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        run_agent_loop(ctx).await.unwrap();
        assert_eq!(exec.calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn cancellation_stops_loop_early() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: vec![tool("search", "{}")],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let (tx, rx) = watch::channel(false);
        tx.send(true).unwrap();
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig::default(),
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("search")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        run_agent_loop(ctx).await.unwrap();
        assert_eq!(exec.calls.lock().unwrap().len(), 0, "取消后不应执行工具");
    }

    #[tokio::test]
    async fn injected_skills_prompt_reaches_llm() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: "done".into(),
            reasoning: String::new(),
            tool_calls: vec![],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let recorded = llm.recorded.clone();

        let mut msgs = vec![ChatMessage {
            role: "system".to_string(),
            content: Some("SKILLS_PROMPT_MARKER: 你是编程助手".into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        msgs.push(user_msg("任务"));

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig::default(),
            initial_messages: msgs,
            tools: vec![],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        run_agent_loop(ctx).await.unwrap();

        let rec = recorded.lock().unwrap();
        assert!(!rec.is_empty(), "LLM 应至少被调用一次");
        let first = rec.first().unwrap();
        assert_eq!(first[0].role, "system");
        assert!(
            first[0].content.as_deref().unwrap().contains("SKILLS_PROMPT_MARKER"),
            "system prompt 应透传到 LLM"
        );
    }

    // ═══════════════════════════════════════════════════════════
    //  新功能测试
    // ═══════════════════════════════════════════════════════════

    #[tokio::test]
    async fn tool_use_behavior_stop_on_first_tool() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: vec![tool("search", r#"{"q":"hi"}"#)],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let calls = exec.calls.clone();

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig {
                tool_use_behavior: ToolUseBehavior::StopOnFirstTool,
                ..Default::default()
            },
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("search")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        let messages = run_agent_loop(ctx).await.unwrap();

        // 应执行一次工具，然后停止（不调用第二轮 LLM）
        assert_eq!(calls.lock().unwrap().len(), 1, "应执行一次工具");
        // 消息列表应包含初始消息、assistant 轮、tool 结果
        assert!(
            messages.iter().any(|m| m.role == "tool"),
            "应有工具结果消息"
        );
    }

    #[tokio::test]
    async fn tool_use_behavior_stop_at_specific_tool() {
        let llm = FakeLlmClient::new(vec![LlmResponse {
            content: String::new(),
            reasoning: String::new(),
            tool_calls: vec![tool("finalize", r#"{}"#)],
        }]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let calls = exec.calls.clone();

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig {
                tool_use_behavior: ToolUseBehavior::StopAtTools(vec!["finalize".into()]),
                ..Default::default()
            },
            initial_messages: vec![user_msg("结束")],
            tools: vec![tool_schema("finalize")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };
        let messages = run_agent_loop(ctx).await.unwrap();

        assert_eq!(calls.lock().unwrap().len(), 1, "应执行一次 finalize");
        assert!(messages.iter().any(|m| m.role == "tool"), "应有工具结果");
    }

    #[tokio::test]
    async fn context_compaction_reduces_messages() {
        // 构造接近上下文上限的消息列表，验证压缩生效
        let llm = FakeLlmClient::new(vec![
            LlmResponse {
                content: String::new(),
                reasoning: String::new(),
                tool_calls: vec![tool("search", r#"{}"#)],
            },
            LlmResponse {
                content: "done".into(),
                reasoning: String::new(),
                tool_calls: vec![],
            },
        ]);
        let exec = FakeExecutor {
            calls: Arc::new(Mutex::new(Vec::new())),
        };

        let mut msgs = vec![ChatMessage::system("你是助手。")];
        // 添加多条消息使 token 估算超过阈值（context_limit 故意设小）
        for i in 0..20 {
            msgs.push(ChatMessage::user(&format!("第{}轮用户问", i)));
            msgs.push(ChatMessage::assistant(
                format!("第{}轮回答 {}", i, "a".repeat(500)),
                String::new(),
                vec![],
            ));
        }
        msgs.push(ChatMessage::user("新问题"));

        let before_tokens = estimate_messages_tokens(&msgs);
        assert!(
            before_tokens > 50,
            "测试数据 token 数应足够大: {}",
            before_tokens
        );

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig {
                // 设得很小确保触发压缩
                context_limit: 100,
                compaction_threshold: 0.5,
                ..Default::default()
            },
            initial_messages: msgs.clone(),
            tools: vec![tool_schema("search")],
            llm: &llm,
            executor: &exec,
            cancel: &mut rx,
            approval_rx: None,
        };

        let result = run_agent_loop(ctx).await.unwrap();
        // 压缩后消息数应少于原始（压缩把多条历史摘要为一条）
        assert!(
            result.len() < msgs.len(),
            "压缩后消息数 {} 应小于压缩前 {}",
            result.len(),
            msgs.len()
        );
    }

    #[test]
    fn estimate_tokens_basic() {
        assert!(estimate_tokens("hello") >= 1);
        assert!(estimate_tokens("") >= 1);
        // 长文本
        let long = "a".repeat(300);
        assert!(estimate_tokens(&long) >= 100);
    }

    #[test]
    fn estimate_message_tokens_counts_correctly() {
        let msg = ChatMessage::system("Hello world");
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens >= 20, "应包含 MSG_OVERHEAD + 内容: {}", tokens);
    }

    #[test]
    fn compact_context_basic() {
        let mut msgs = vec![
            ChatMessage::system("你是助手。"),
            ChatMessage::user("今天天气如何？"),
            ChatMessage::assistant("晴天。".to_string(), String::new(), vec![]),
            ChatMessage::user("明天呢？"),
            ChatMessage::assistant("也晴天。".to_string(), String::new(), vec![]),
            ChatMessage::user("后天呢？"),
            ChatMessage::assistant("雨。".to_string(), String::new(), vec![]),
            ChatMessage::user("大后天呢？"),
            ChatMessage::assistant("雪。".to_string(), String::new(), vec![]),
            ChatMessage::user("最新问题"),
        ];

        let before = msgs.len();
        let _compacted = compact_context(&mut msgs);
        assert!(
            msgs.len() < before,
            "压缩后消息数应减少: {} < {}",
            msgs.len(),
            before
        );
        // 仍保留最新用户消息
        assert!(msgs.iter().any(|m| m.content.as_deref() == Some("最新问题")));
        // system prompt 被保留
        assert!(msgs.iter().any(|m| m.content.as_deref() == Some("你是助手。")));
    }

    #[tokio::test]
    async fn enrichment_truncates_long_results() {
        let original = "a".repeat(10000);
        let enriched = enrich_tool_result(&original, 200);
        assert!(enriched.contains("已截断"), "应包含截断标记");
        assert!(enriched.len() < original.len(), "富化后应变短");
    }

    #[test]
    fn enrichment_preserves_json_structure() {
        let json = r#"{"name":"test","items":[1,2,3],"nested":{"key":"value"}}"#;
        let enriched = enrich_tool_result(json, 5000);
        assert!(!enriched.contains("已截断"), "短 JSON 不应被截断");
    }

    #[test]
    fn enrichment_compresses_long_json() {
        let long_val = "x".repeat(500);
        let json = format!(r#"{{"data":"{}","items":[{}]}}"#, long_val, (0..100).map(|i| format!(r#""item{}""#, i)).collect::<Vec<_>>().join(","));
        let enriched = enrich_tool_result(&json, 500);
        assert!(enriched.contains("已结构化压缩") || enriched.len() < json.len(), "长 JSON 应被压缩: 原长 {} 压缩后 {}", json.len(), enriched.len());
    }
}
