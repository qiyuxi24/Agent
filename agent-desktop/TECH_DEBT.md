# 技术债务清单 (TECH DEBT)

> 最后整理时间：2026-07-14
> 范围：`agent-desktop/src-tauri/src/` 下的所有 Rust 模块
> 关联文档：`TODO.md`（功能规划）、`README.md`（项目说明）

本文件记录代码质量扫描发现的问题、已完成的修复、以及尚未处理（低优先级）的债务。
状态标记：`✅ 已修复` / `⏳ 待处理` / `🔍 已识别待评估`

---

## 一、扫描范围与方法

对以下文件做了逐文件静态扫描，识别 7 类技术债务：

- **耦合问题**：模块间过度依赖、硬编码路径/常量、应注入却用全局状态
- **错误处理**：`unwrap()`/`expect()` 可能 panic、`String` 作错误类型、错误信息不清
- **代码重复**：跨文件/同文件重复逻辑
- **未完成的 TODO/FIXME/HACK**
- **性能问题**：不必要的 clone、async 上下文中阻塞、锁粒度过粗
- **可维护性**：魔术数字、过长函数、过深嵌套、注释掉的代码、命名不一致
- **安全性**：敏感信息泄露到日志、未验证输入

扫描文件：`lib.rs` · `agent_loop.rs` · `mcp.rs` · `skills.rs` · `rag.rs` · `browser.rs` · `error_codes.rs` · `plugins.rs` · `code_server.rs` · `ide.rs`

---

## 二、已修复债务（2026-07-14，commit `b2e6d39`）

### P0 — 稳定性（4 处）

| # | 文件:行 | 严重问题 | 修复方式 | 影响 |
|---|---------|----------|----------|------|
| 1 | `mcp.rs:210` | `std_cmd.creation_flags(0x08000000)` 无条件编译守卫，非 Windows 平台编译失败 | 加 `#[cfg(windows)]` 守卫 | 跨平台可编译 |
| 2 | `mcp.rs:436` | stderr 日志截断 `&line_str[..line_str.len().min(120)]` 可能切到多字节 UTF-8 字符中间导致 panic | 用 `is_char_boundary()` 向前找最近字符边界 | 中文/多字节日志不再 panic |
| 3 | `lib.rs:277` | HTTP 错误时 `response.text().await.unwrap_or_default()` 静默吞掉 body 读取失败，丢失诊断信息 | 改为 `?` 传播，携带 `(无法读取响应体: {e})` | 错误诊断更完整 |
| 4 | `lib.rs:662` | `create_dir_all` 失败时 `unwrap_or_else` 仅打印，且之后仍误打印「已创建数据目录」 | `if let Err(e)` 分支处理，成功才打印 | 启动失败不再误报成功 |

### P1 — 性能 / 耦合（8 处）

| # | 文件:行 | 问题 | 修复方式 | 收益 |
|---|---------|------|----------|------|
| 5 | `lib.rs:253` | `run_completion` 每次调用 `reqwest::Client::new()`，重复建立 TCP 连接 | `static LazyLock<reqwest::Client>` 全局复用连接池 | 减少握手开销 |
| 6 | `mcp.rs:664` | `llm_tools()` 每次遍历所有 server/tool 重建 `Vec<Value>` | 加 `tools_cache: Mutex<Option<(Instant, Vec<Value>)>>`，30s TTL；`connect`/`disconnect` 时 `invalidate_tools_cache()` | 减少重复构建 |
| 7 | `lib.rs` + `agent_loop.rs` | `ChatMessage { role: "system", content: Some(...), ... }` 重复构造 5 处 | `impl ChatMessage` 新增 `::system()` / `::user()` / `::tool()` / `::assistant()` 构造函数 | 消除重复、统一字段逻辑 |
| 8 | `lib.rs` | `10` / `1` / `0.7` / `4096` / `"chat"` 魔术数字散布 | 提取常量 `AGENT_MAX_ITERATIONS` / `CHAT_MAX_ITERATIONS` / `DEFAULT_TEMPERATURE` / `DEFAULT_MAX_TOKENS` / `CANCEL_STREAM_KEY` | 可维护性提升 |
| 9 | `mcp.rs` + `plugins.rs` | 5 处相同 `reqwest::Client::builder().user_agent("agent-desktop/0.3.0")...` 构建模式 | `lib.rs` 新增 `pub(crate) build_market_client(timeout_secs)` + `pub(crate) const USER_AGENT` | 消除重复、UA 统一 |
| 10 | `mcp.rs` | `llm_tools` 缓存失效时机缺失 | `connect()`/`disconnect()` 末尾调用 `invalidate_tools_cache().await` | 缓存一致性 |
| 11 | `mcp.rs` | `tool_cache` 类型误写（短暂把 val 改为 `Vec<Value>`） | 已修正回 `HashMap<String, (Instant, String)>` | 类型正确 |
| 12 | `lib.rs` | `cancel_chat` / `cleanup` 中硬编码 `"chat"` key | 改用 `CANCEL_STREAM_KEY` 常量 | 单一来源 |

### P2 — 可维护性（3 处）

| # | 文件:行 | 问题 | 修复方式 | 收益 |
|---|---------|------|----------|------|
| 13 | `agent_loop.rs:435` | `sanitize_args` JSON 序列化失败时 `unwrap_or_else(\|_\| args.to_string())` 回退到原始参数，可能泄露敏感字段 | 改为返回 `"(redacted params, serialization failed: {e})"` | 日志脱敏安全 |
| 14 | `agent_loop.rs:428` | 敏感字段列表 `["key","secret","token",...]` 硬编码在函数内 | 提取 `const SENSITIVE_FIELDS: &[&str]` | 集中管理 |
| 15 | `agent_loop.rs:176-309` | `run_agent_loop` 约 135 行（THINK + ACT/OBSERVE 混合） | 提取 `execute_and_observe()` 独立函数，主循环降至约 80 行 | 单一职责、易读易测 |

### 验证结果
- `cargo check` — 零新错误（仅 code-server 原生模块缺失的既有警告）
- `cargo test --lib` — 7/7 单测通过
- `read_lints` — 零 warning

---

## 三、待处理债务（低优先级，未触及）

> 这些项功能正确，修复收益相对低，或改动风险较高，留待后续迭代。

### ⏳ 锁粒度（中等优先级）
- **`mcp.rs:call_namespaced`**：持有 `servers` 锁的时间跨越「工具调用 + 缓存写入」，可能阻塞其他操作（如 `llm_tools` 读取）。建议改为「获取 client 引用后释放锁，再执行调用」。

### ⏳ 重复模式（低优先级）
- **`plugins.rs` 与 `mcp.rs` 的 OnceLock 缓存**：`market_cache()` 模式重复，可提取为通用 `TtlCache<K, V>` trait/util。
- **`agent_loop.rs:383-386`**：每次迭代 `valid_tools.iter().cloned().collect()`，开销极小，可忽略。

### ⏳ 函数复杂度（低优先级）
- **`lib.rs:msg_to_value`**：手动构建 JSON map 略冗长，功能正确，暂不重构。
- **`lib.rs:run_completion`**：约 128 行，嵌套深（loop > match > if let）。可进一步拆分流式解析逻辑，但当前可读。

### ⏳ 硬编码（低优先级）
- **`mcp.rs` 多处 `"version": "0.3.0"`**：MCP 协议版本号硬编码，建议从 `Cargo.toml` 或常量读取（与 `USER_AGENT` 同步）。
- **`agent_loop.rs:67-72`**：`AgentLoopConfig` Default 中的 `10/8000/300/3/2` 已有字段名说明，可加文档注释说明选择依据。

### ⏳ 输入校验（低优先级）
- **`skills.rs` / `rag.rs`**：外部技能/文档路径未做严格沙箱校验，当前依赖 Tauri 命令层可信调用方，建议增加路径穿越防护。

---

## 四、修复原则（本次遵循）

1. **最小侵入**：不改动现有事件协议（`stream-token`/`tool-call`/`tool-result` 等）与取消机制，前端零影响。
2. **稳定性优先**：P0（panic/编译失败/数据丢失风险）先于 P1/P2 处理。
3. **去耦合方向**：提取构造函数、工厂函数、常量，把重复逻辑收敛到单一来源。
4. **可测试性**：所有重构保持原有 7 个单测通过，新增修复不降低覆盖率。
5. **不引入新 crate**：仅用现有 `tokio` / `serde_json` / `std::sync::LazyLock`（Rust 1.80+ 稳定）。

---

## 五、后续建议

- **P0 项已全部清零**，当前代码无已知 panic/编译风险。
- 建议将「每次 `cargo check` + `cargo test` 通过」纳入 pre-commit（项目已有 husky + lint-staged，可扩展）。
- 剩余债务可按「锁粒度 → 缓存 util → 路径校验」顺序在后续迭代处理，单次 PR 聚焦一类。

---

## 六、架构审查发现（2026-07-15）

> 本次审查聚焦模块边界、文件大小、耦合模式和重复模式，不深入每个文件的实现细节。

### P0 — 文件规模 / 模块拆分（2 项，正在修复）

| # | 文件 | 行数 | 问题 | 修复方案 |
|---|------|------|------|----------|
| 16 | `mcp.rs` | 1336 | 单文件承载 4 种职责：类型定义、McpClient 实现、McpManager 管理、市场数据抓取。新增功能无处安放。 | 拆为 `mcp/types.rs` + `client.rs` + `manager.rs` + `market.rs`，原 `mcp.rs` 改为 `mcp/mod.rs` 重导出 |
| 17 | `lib.rs` | 984 | 类型定义（ChatRequest/ChatMessage/ToolCall/流事件）与业务逻辑（chat_stream/run_completion）与 AppState 混在同一文件，是典型的 God Object。 | 提取 `types.rs`（~180 行）：ChatRequest、ChatMessage、ToolCall、所有 Stream* 事件、ToolCallAcc、msg_to_value、finalize_tool_calls |

### P1 — 重复模式 / 耦合（3 项）

| # | 文件 | 问题 | 建议 |
|---|------|------|------|
| 18 | `MarketPanel.tsx` (两种) | Skills 市场和 MCP 市场面板结构高度相似（搜索→列表→安装按钮），各自独立实现。 | 提取通用 `<MarketPanel>` 组件，通过 props/children 差异化内容 |
| 19 | `appStore.ts` | Zustand store 承载 12+ 种不同领域状态（对话/设置/UI/技能/MCP/RAG/插件/主题等），超过 400 行。 | 按领域拆分为独立 slice（chatSlice/settingsSlice/uiSlice/...），用 `persist` 中间件按需持久化 |
| 20 | `ide.rs` | IDE 内核（文件操作/代码执行/终端）与 code_server 存在功能边界模糊：部分命令（ide_execute_code）在 ide.rs 实现，但 code_server 内置执行能力。 | 明确边界：ide.rs = 本地轻量操作（文件/目录/搜索）；code_server.rs = VS Code 内核能力（编辑器/诊断/终端）。删除重叠路径。 |

### P2 — 可维护性 / 一致性（3 项）

| # | 问题 | 建议 |
|---|------|------|
| 21 | **版本号不一致**：`Cargo.toml` → `0.3.0`，`USER_AGENT` → `0.3.0`，`mcp.rs` MCP 协议中 `"version": "0.3.0"`，`branding.json` → `0.3.0`。四处分散，手动同步易遗漏。 | 从 `Cargo.toml` 读取或从 `branding.json` 单一来源读取版本号 |
| 22 | **市场数据回退硬编码**：`mcp.rs:builtin_mcp_market()` 和 `skills.rs` 内置数据均硬编码在 Rust 源码中，更新需重新编译。 | 将离线回退数据移入 JSON 资源文件（embed 到二进制或从 app_data 读取） |
| 23 | **前端测试零覆盖**：Vite 模板默认测试文件被删，项目中无任何前端测试。 | 至少为关键业务逻辑（消息格式化、i18n 切换、API key 加密）添加 Vitest 单元测试 |

## 七、本轮修复（2026-07-16）

| 领域 | 问题 | 修复方式 |
|------|------|----------|
| 版本号一致性 | 6 处硬编码版本号（`0.3.0` / `0.3` / `0.1`）互不一致 | 全部改为 `env!("CARGO_PKG_VERSION")` 编译时宏，统一从 `Cargo.toml` 派生；Cargo.toml 同步 `0.3.0` |
| MCP 锁粒度 | `call_namespaced` 持久 `servers` 锁跨越 `call_tool` 异步 I/O | 改用 `HashMap::remove/insert` 暂移 client，锁仅在查/写 HashMap 时持有，异步调用不锁 |
| 路径穿越 | `skills_delete/toggle/read_content/install` 的 `id` 参数未校验 | 新增 `validate_skill_id()`，拒绝含 `/`、`\`、`..`、非字母数字的 id |
| 函数复杂度 | `run_completion` 219 行，SSE 解析与 HTTP 请求耦合 | 提取 `parse_sse_response()`（独立 SSE 解析）+ `detect_quota_error()`（独立检测） |
| User-Agent 过时 | `skills.rs` 两处过时 UA（`agent-desktop/0.3.0`、`votek/0.1`） | 统一为 `concat!("votek/", env!("CARGO_PKG_VERSION"))` |
| 窗口权限 | `capabilities/default.json` 缺少 `allow-maximize` 权限 | 添加 `allow-maximize` 和 `allow-unmaximize` |

### 更新已修复列表

以下待处理债务项目已在本轮修复，从待处理清单中移除：
- ~~锁粒度：`mcp.rs:call_namespaced`~~ → ✅ 已修复
- ~~硬编码版本号~~ → ✅ 已统一
- ~~输入校验：skills.rs~~ → ✅ 已添加路径穿越防护
- ~~`run_completion` 拆分~~ → ✅ SSE 解析已提取为独立函数

### 新发现预存问题（非本轮引入）

| # | 文件 | 问题 | 影响 |
|---|------|------|------|
| 24 | `code_server.rs` | Tauri v2.11 API 变更：`webviews()` 返回 `Vec<(String, Webview)>` 非 `Vec<Webview>`；`size()` 方法移除 | `cargo test --lib` 编译失败（`cargo check` 因缓存通过） |
| 25 | `build.rs` | Tauri v2.11 权限校验：`capabilities/default.json` 中存在若干未注册的权限 | `cargo build` 自定义脚本失败 |
| 26 | `TODO.md:248` | 前端 `ChatView.tsx` 有两套重复的思考事件处理代码（Tauri Event 监听 + fetch SSE 回调） | 逻辑重复，维护成本高 |
