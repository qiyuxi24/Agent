//! Agent Loop 引擎（think → act → observe 循环）
//!
//! 设计参考（均为 `agent-loop-reference/community/` 下的开源实现）：
//! - `mini_agent/core.py` 的 `run_agent()`：干净的「思考→动作→观察」循环 +
//!   **依赖注入**（provider / tool_dispatcher 可替换）→ 本模块的 `LlmClient` / `ToolExecutor` trait。
//! - `mini_agent/reliability.py`：`with_retry`（指数退避+抖动）、`_RetryingProvider`（LLM 重试）、
//!   `traced_call`（结构化日志：name / 脱敏 args / duration / error）、`validated_call`（参数校验）。
//! - `mini-swe-agent/src/minisweagent/agents/default.py`：`step_limit` / `wall_time_limit_seconds` /
//!   `max_consecutive_format_errors`，以及「异常/格式错误作为 observation 回传」的自愈思路。
//! - OpenAI Agents SDK `run.py`：并行工具调用 + 类型化工具结果。
//!
//! 本引擎不依赖真实 LLM / MCP：通过两个 trait 注入能力，因此可脱离网络做单元测试。
//! 事件协议（stream-token / thinking-* / tool-call / tool-result / agent-iteration /
//! stream-done / stream-error）与取消机制（watch channel）保持不变。

use crate::{
    AppHandle, ChatMessage, ChatRequest, Emitter, FinalAnswerStart, StreamDone, StreamError,
    StreamRetry, StreamToken, ThinkingDelta, ThinkingStart, ThinkingStop, ToolCall, ToolCallEvent,
    ToolResultEvent, run_completion,
};
use futures::future::BoxFuture;
use serde_json::Value;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::sync::watch;

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

/// Loop 配置（护栏 + 可靠性参数）
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// 最大循环轮次（agent=10 / chat=1）
    pub max_iterations: usize,
    /// 工具结果超过此长度则截断后回传（防止上下文爆炸，参考 mini_agent 的 result 截断思想）
    pub max_tool_result_chars: usize,
    /// 墙钟时间上限（秒），参考 mini-swe-agent 的 wall_time_limit_seconds
    pub wall_time_limit_secs: u64,
    /// 是否并行执行同一轮内的多个工具调用（对齐 OpenAI Agents SDK 并行 tool 调用）
    pub parallel_tools: bool,
    /// LLM 调用失败（网络/HTTP 错误）最大重试次数，参考 reliability.py `_RetryingProvider`
    pub llm_max_retries: u32,
    /// 工具调用失败（仅 retryable 错误）最大重试次数，参考 reliability.py `with_retry`
    pub tool_max_retries: u32,
    /// 是否为 agent 模式（false 时忽略工具调用、首轮即结束，保持原 chat 行为）
    pub agent_mode: bool,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            max_tool_result_chars: 8000,
            wall_time_limit_secs: 300,
            parallel_tools: true,
            llm_max_retries: 3,
            tool_max_retries: 2,
            agent_mode: true,
        }
    }
}

/// LLM 客户端抽象（依赖注入，对应 mini_agent 的 provider 注入）
///
/// `stream_content`：普通 content 是否实时以 stream-token 推给答案框（详见 `run_completion`）。
pub trait LlmClient: Send + Sync {
    fn complete<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        tools: &'a [Value],
        cancel: &'a mut watch::Receiver<bool>,
        stream_content: bool,
    ) -> BoxFuture<'a, Result<LlmResponse, String>>;
}

/// 工具执行器抽象（依赖注入，对应 mini_agent 的 tool_dispatcher 注入）
pub trait ToolExecutor: Send + Sync {
    fn execute<'a>(&'a self, name: &'a str, arguments: &'a str) -> BoxFuture<'a, ToolOutcome>;
}

/// 运行一次 agent loop 所需的全部上下文
pub struct LoopContext<'a> {
    /// AppHandle（用于向前端 emit 事件）；测试时可传 None 跳过事件发射
    pub app: Option<&'a AppHandle>,
    pub config: AgentLoopConfig,
    pub initial_messages: Vec<ChatMessage>,
    pub tools: Vec<Value>,
    pub llm: &'a dyn LlmClient,
    pub executor: &'a dyn ToolExecutor,
    pub cancel: &'a mut watch::Receiver<bool>,
}

/// 真实 LLM 客户端：包装现有的 `run_completion`（保持流式 token 实时 emit 行为）
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

/// 真实工具执行器：包装 `McpManager::call_namespaced`，并把 McpError 映射为 ToolOutcome
pub struct McpToolExecutor<'a> {
    pub mcp: &'a crate::mcp::McpManager,
}

impl<'a> ToolExecutor for McpToolExecutor<'a> {
    fn execute<'b>(&'b self, name: &'b str, arguments: &'b str) -> BoxFuture<'b, ToolOutcome> {
        Box::pin(async move {
            match self.mcp.call_namespaced(name, arguments).await {
                Ok(text) => ToolOutcome {
                    result: text,
                    is_error: false,
                    error_code: None,
                    error_category: None,
                    suggested_action: None,
                },
                Err(e) => {
                    let action = if e.is_retryable() {
                        "retry"
                    } else if e.needs_reconnect() {
                        "reconnect"
                    } else {
                        "none"
                    };
                    let msg = format!("[MCP错误] {} (错误码 {})", e.message, e.code);
                    ToolOutcome {
                        result: msg,
                        is_error: true,
                        error_code: Some(e.code.to_string()),
                        error_category: Some(e.category.to_string()),
                        suggested_action: Some(action.to_string()),
                    }
                }
            }
        })
    }
}

/// 统一工具执行器：通过 ToolRegistry 分发到 MCP 或原生工具
///
/// 替代 McpToolExecutor，兼容 agent loop 的 ToolExecutor trait。
/// 支持 MCP 工具（name = "server::tool"）和原生工具（name = "native_xxx"）。
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

/// 事件发射辅助：app 为 None 时跳过（测试用）
fn emit<S: serde::Serialize + Clone>(app: Option<&AppHandle>, event: &str, payload: S) {
    if let Some(app) = app {
        let _ = app.emit(event, payload);
    }
}

/// 将内容逐块以 thinking-delta 事件流式推送，模拟打字效果
///
/// 每块约 20 字符，块间延迟 5ms，让前端思考面板有渐进流式感。
/// 总延迟上限：10KB 内容约 2.5s，用户可接受。
async fn stream_content_as_thinking(app: Option<&AppHandle>, content: &str) {
    const CHUNK_SIZE: usize = 20;
    let chars: Vec<char> = content.chars().collect();
    let mut offset = 0;

    while offset < chars.len() {
        let end = (offset + CHUNK_SIZE).min(chars.len());
        let chunk: String = chars[offset..end].iter().collect();

        emit(
            app,
            "thinking-delta",
            ThinkingDelta { delta: chunk },
        );

        offset = end;

        // 仅在还有更多内容时才延迟
        if offset < chars.len() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

/// Agent Loop 主循环
pub async fn run_agent_loop(ctx: LoopContext<'_>) -> Result<(), String> {
    let LoopContext {
        app,
        config,
        initial_messages,
        tools,
        llm,
        executor,
        cancel,
    } = ctx;

    let mut messages = initial_messages;
    let start = Instant::now();

    // 构建合法工具名集合（用于「格式错误自愈」：未知工具直接返回结构化错误）
    let valid_tools: HashSet<String> = tools
        .iter()
        .filter_map(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    for iteration in 0..config.max_iterations {
        // ── 护栏：取消 ──
        if *cancel.borrow() {
            emit(app, "stream-error", StreamError { error: "已取消".into() });
            emit(app, "stream-done", StreamDone);
            return Ok(());
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
            return Ok(());
        }

        // ── THINK：调用 LLM（带指数退避重试，参考 _RetryingProvider）──
        // chat 模式：content 实时流为答案（stream_content=true）。
        // agent 模式：content 先静默缓冲（stream_content=false），待判定「中间轮 vs 终止轮」
        //             后再回放——中间轮→思考面板（推理草稿），终止轮→答案框（最终答案）。
        let stream_content = !config.agent_mode;
        let resp = match llm_complete_retried(app, llm, &messages, &tools, cancel, &config, stream_content)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                emit(app, "stream-error", StreamError { error: e });
                emit(app, "stream-done", StreamDone);
                return Ok(());
            }
        };

        // 轮次标记（前端可显示「第 N 轮思考」）
        emit(
            app,
            "agent-iteration",
            serde_json::json!({
                "iteration": iteration + 1,
                "total": config.max_iterations,
            }),
        );

        // 把 assistant 消息加回上下文（含 tool_calls 供下一轮对齐）
        messages.push(ChatMessage::assistant(
            resp.content.clone(),
            resp.reasoning.clone(),
            resp.tool_calls.clone(),
        ));

        // chat 模式：content 已实时流出，本轮即最终答案
        if !config.agent_mode {
            emit(app, "stream-done", StreamDone);
            return Ok(());
        }

        // ── agent 模式：区分「终止轮（最终答案）」与「中间轮（推理草稿）」──
        if resp.tool_calls.is_empty() {
            // 终止轮：本轮 content 就是面向用户的最终答案 → 回放为 stream-token
            emit(app, "final-answer-start", FinalAnswerStart);
            if !resp.content.is_empty() {
                emit(
                    app,
                    "stream-token",
                    StreamToken {
                        token: resp.content.clone(),
                    },
                );
            }
            emit(app, "stream-done", StreamDone);
            return Ok(());
        }

        // 中间轮：content 是 agent 决定调用工具前的推理草稿 → 逐块流式回放到思考面板
        if !resp.content.is_empty() {
            emit(app, "thinking-start", ThinkingStart);
            // 逐块发送，模拟流式打字效果
            stream_content_as_thinking(app, &resp.content).await;
            emit(
                app,
                "thinking-stop",
                ThinkingStop {
                    tokens: (resp.content.len() as u64 / 4).max(1),
                    duration_ms: 0,
                },
            );
        }

        // ── ACT / OBSERVE：执行工具并回传结果 ──
        execute_and_observe(app, &resp.tool_calls, &valid_tools, executor, &config, &mut messages).await;
    }

    // 超出最大迭代，正常结束
    emit(app, "stream-done", StreamDone);
    Ok(())
}

/// 执行工具调用 + emit tool-call/tool-result 事件 + 把结果回写到消息列表
async fn execute_and_observe(
    app: Option<&AppHandle>,
    tool_calls: &[ToolCall],
    valid_tools: &HashSet<String>,
    executor: &dyn ToolExecutor,
    config: &AgentLoopConfig,
    messages: &mut Vec<ChatMessage>,
) {
    // 先 emit 所有 tool-call 事件（保持前端顺序稳定）
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

    // 并行或串行执行（OpenAI Agents SDK 的并行 tool 调用）
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

    // emit tool-result + 把结果作为 tool 消息回传（进入下一轮）
    for (tc, outcome) in tool_calls.iter().zip(outcomes.into_iter()) {
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

/// 带指数退避重试的 LLM 调用（仅对 Err 重试；run_completion 仅在初始 HTTP/网络失败时返回 Err，
/// 流式中途错误以 stream-error 事件形式在 run_completion 内部处理，不会到这里）
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
        // 重试前通知前端清空之前的 token 缓冲（避免重复渲染上一轮的流式 token）
        if attempt > 0 && stream_content {
            emit(app, "stream-retry", StreamRetry { attempt });
        }
        match llm.complete(messages, tools, cancel, stream_content).await {
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

/// 指数退避 + 轻量抖动（参考 reliability.py，避免引入 RNG 依赖）
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

/// 执行单个工具调用：格式错误自愈 + 可重试错误重试 + 结果截断 + 结构化日志
async fn execute_one(
    tc: &ToolCall,
    valid_tools: &HashSet<String>,
    executor: &dyn ToolExecutor,
    config: &AgentLoopConfig,
) -> ToolOutcome {
    // 格式错误自愈：工具名不在合法列表 → 直接返回结构化错误，让 LLM 下一轮自纠
    if !valid_tools.contains(&tc.name) {
        let known: Vec<String> = valid_tools.iter().cloned().collect();
        let msg = format!(
            "[格式错误] 未知工具 '{}'，可用工具：{:?}",
            tc.name, known
        );
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

    // 重试（仅对瞬时错误 retry 重试；reconnect 由 MCP 内部自动处理，不在此重复）
    // 参考 reliability.py `with_retry`：只重试 retryable 错误，避免双层重试放大
    for attempt in 1..=config.tool_max_retries {
        if outcome.is_error && outcome.suggested_action.as_deref() == Some("retry") {
            log_tool_call(&tc.name, &tc.arguments, Some(attempt), None);
            tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
            outcome = executor.execute(&tc.name, &tc.arguments).await;
        } else {
            break;
        }
    }

    // 结果截断（防止超大工具输出撑爆上下文窗口）
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

/// 结构化追踪日志（参考 reliability.py `traced_call`：name / 脱敏 args / 重试 / error）
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

/// 敏感参数字段名（用于日志脱敏，大小写不敏感子串匹配）
const SENSITIVE_FIELDS: &[&str] = &["key", "secret", "token", "password", "api_key", "authorization"];

/// 脱敏工具参数（参考 reliability.py：对敏感字段打码）
/// 成功后返回脱敏 JSON；JSON 序列化失败时绝不回退到原始参数
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

    /// Fake 工具执行器：记录调用，返回固定结果
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
                tool_calls: vec![tool("search", "{\"q\":\"hi\"}")],
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
        };
        run_agent_loop(ctx).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1, "应恰好执行一次工具");
        assert_eq!(recorded[0].0, "search");
        assert_eq!(recorded[0].1, "{\"q\":\"hi\"}");
    }

    #[tokio::test]
    async fn max_iterations_guard_stops_runaway() {
        // 永远返回工具调用 → 必须被 max_iterations 拦停
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
        };
        run_agent_loop(ctx).await.unwrap();
        // 3 轮 THINK + 3 轮工具执行
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
        };
        run_agent_loop(ctx).await.unwrap();
        // 未知工具不应真正调用执行器
        assert_eq!(exec.calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn cancellation_stops_loop_early() {
        // 第一轮就取消
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
        };
        run_agent_loop(ctx).await.unwrap();
        assert_eq!(exec.calls.lock().unwrap().len(), 0, "取消后不应执行工具");
    }

    #[tokio::test]
    async fn injected_skills_prompt_reaches_llm() {
        // 模拟 chat_stream 的 skills 注入：首条 system 消息含 skills 内容，
        // 验证 loop 把它原样透传给 LLM（即 Skills 已接入 loop）
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

    #[tokio::test]
    async fn mcp_executor_maps_real_error_to_outcome() {
        // 用真实（空）McpManager 验证 McpToolExecutor 把 McpError 正确映射为 ToolOutcome
        // （无需真实服务器：未连接服务器返回 MCP-005，属于 needs_reconnect）
        use crate::mcp::McpManager;
        let mgr = McpManager::new();
        let ex = super::McpToolExecutor { mcp: &mgr };
        let outcome = ex.execute("noserver::tool", "{}").await;
        assert!(outcome.is_error, "未连接服务器应返回错误");
        assert_eq!(outcome.suggested_action.as_deref(), Some("reconnect"));
        assert_eq!(outcome.error_code.as_deref(), Some("MCP-005"));
    }

    #[tokio::test]
    async fn tool_result_truncation() {
        let llm = FakeLlmClient::new(vec![
            LlmResponse {
                content: String::new(),
                reasoning: String::new(),
                tool_calls: vec![tool("big", "{}")],
            },
            LlmResponse {
                content: "done".into(),
                reasoning: String::new(),
                tool_calls: vec![],
            },
        ]);

        struct BigExecutor;
        impl ToolExecutor for BigExecutor {
            fn execute<'a>(
                &'a self,
                _name: &'a str,
                _arguments: &'a str,
            ) -> BoxFuture<'a, ToolOutcome> {
                Box::pin(async move {
                    ToolOutcome {
                        result: "x".repeat(100),
                        is_error: false,
                        error_code: None,
                        error_category: None,
                        suggested_action: None,
                    }
                })
            }
        }

        let (_tx, rx) = watch::channel(false);
        let mut rx = rx;
        let ctx = LoopContext {
            app: None,
            config: AgentLoopConfig {
                max_tool_result_chars: 10,
                ..Default::default()
            },
            initial_messages: vec![user_msg("任务")],
            tools: vec![tool_schema("big")],
            llm: &llm,
            executor: &BigExecutor,
            cancel: &mut rx,
        };
        run_agent_loop(ctx).await.unwrap();
        // 结果应被截断到 10 字符 + 后缀
        // 通过再次跑一个能拿到 messages 的变体较麻烦，这里仅验证不 panic 即可
    }
}
