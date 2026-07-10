# Agent Loop 参考集

> 独立的学习/参考模块，**不接入现有工程**（agent-desktop）。
> 目的：先复用业界成熟的开源 agent loop，再配一份自己手写的极简实现方便理解原理。

---

## 一、什么是 Agent Loop

剥掉所有花哨外壳，一个 AI Agent 的核心就是一个循环：

```
messages = [system_prompt, user_task]
while True:
    output = call_llm(messages)          # 1. 调模型（思考）
    messages.append(assistant: output)
    action = parse_or_get_tool_call(output)  # 2. 拿到要执行的动作/工具调用
    if action is None:  break            # 3. 没有动作 => 任务结束
    result = execute(action)             # 4. 执行工具
    messages.append(tool/user: result)   # 5. 把结果回传，进入下一轮
```

区别只在于「动作怎么表达」和「工具怎么执行」这两点。

---

## 二、开源 Agent Loop 调研（2025-2026）

| 项目 | 语言 | 动作表达方式 | 特点 | 适合场景 |
|------|------|--------------|------|----------|
| **mini-swe-agent** (`swe-agent/mini-swe-agent`) | Python | 文本里用 ```` ```bash ```` 代码块，正则解析 | 官方号称 ~100 行核心，SWE-bench 能到 74%；不依赖 tool-calling API，任意模型可用 | 学习原理 / 代码修复类任务 |
| **OpenAI Agents SDK** (`openai/openai-agents-python`) | Python | 原生 function calling（tools 参数） | 官方内置 agent loop，自动处理工具调用、结果回传、handoff、guardrails | 生产级、需要多 agent 协作 |
| **Claude Agent SDK** (Anthropic) | Python/TS | 原生 tool use | Anthropic 官方，前身 Claude Code SDK；强调上下文管理与子 agent | 生产级、Claude 生态 |
| **minimal-agent** (`Antropath/minimal-agent`) | Python | 让模型直接写 Python 代码当动作 | 教育向，展示"代码即动作"（CodeAct）范式 | 学习 CodeAct |
| **mini_agent** (`sergenes/mini_agent`) | Python | OpenAI SDK + while 循环 | 纯手写、零框架，配套 Medium 系列文章 | 入门教程 |
| **smolagents** (HuggingFace) | Python | CodeAct（写代码执行动作） | 轻量、HF 生态、支持工具与沙箱 | 轻量工具型 agent |
| **LangGraph** (LangChain) | Python/TS | 图 (graph) 编排节点 | 把 loop 表达成状态图，可分支/循环/持久化 | 复杂多步、需要可控流程 |
| **AutoGen** (Microsoft) | Python | 多 agent 对话 | 多代理协作，group chat | 多 agent 场景 |
| **CrewAI** | Python | 角色化 agent + task | Role/Task/Crew 抽象，团队协作 | 角色分工型 |

### 两大流派

1. **文本解析派**（mini-swe-agent / minimal-agent / smolagents）
   - 模型把动作写在回复文本里（代码块 / XML / Python 代码），程序用正则或执行器提取。
   - 优点：任何模型都能用，不依赖 API 的 function calling 能力，可控、易调试。
   - 缺点：需要自己写解析和纠错。

2. **原生 Tool-Calling 派**（OpenAI Agents SDK / Claude SDK / 本仓库 `my_agent_loop.py`）
   - 模型通过 API 的 `tools` / `function_call` 结构化字段返回工具调用。
   - 优点：结构化、稳定、多工具并行、参数校验方便。
   - 缺点：依赖模型/服务端支持 function calling。

---

## 三、本目录内容

```
agent-loop-reference/
├── README.md                    # 本文件：调研 + 说明
├── requirements.txt             # 依赖（openai SDK）
├── reused/
│   └── mini_agent.py            # 复用 mini-swe-agent 蓝图的极简实现（文本解析派）
├── my_agent_loop.py             # 我手写的 tool-calling agent loop（原生工具调用派）
├── concurrent_agent.py          # 多任务并发 agent 池（线程池版 ThreadPoolExecutor）
├── concurrent_agent_async.py    # 多任务并发 agent 池（asyncio + Semaphore 对照版）
└── community/             # 第三方开源仓库（--depth 1 浅克隆，仅本地参考，已 gitignore）
    ├── mini_agent/            # sergenes/mini_agent
    ├── minimal-agent/        # Antropath/minimal-agent
    ├── mini-swe-agent/       # swe-agent/mini-swe-agent
    └── openai-agents-python/ # openai/openai-agents-python（sparse 仅拉 src/agents 核心包）
```

### 自己写的两份参考实现
- `reused/mini_agent.py`：直接复刻 [mini-swe-agent 教程](https://minimal-agent.com/) 的蓝图，
  模型用 ```` ```bash ```` 代码块表达动作，程序执行 shell 并回传。约 90 行，含异常/格式纠错。
- `my_agent_loop.py`：我自己写的，基于 **OpenAI 兼容 function calling**，
  内置 3 个示例工具（计算器 / 读文件 / 列目录），展示结构化工具调用的完整闭环。

### 已下载的第三方仓库（重点看哪些文件）

| 仓库 | 风格 | 核心 agent loop 文件（从哪里看起） |
|------|------|--------------------------------------|
| `community/mini_agent/` | 文本解析派（最简） | `agent.py` + `core.py`：纯手写 while 循环 + OpenAI SDK |
| `community/minimal-agent/` | CodeAct（写代码当动作） | `run_agent.py` + `src/`：把模型输出当 Python 执行 |
| `community/mini-swe-agent/` | 文本解析派（研究级） | `src/minisweagent/agents/default.py`（`run` 里的大循环）、`run/mini.py` |
| `community/openai-agents-python/` | 原生 tool-calling 派（生产级） | `src/agents/run.py`（`Runner.run` 主循环）+ `src/agents/agent.py` |

> 注：openai-agents-python 用 `git clone --filter=blob:none --sparse` 只拉了 `src/agents` 核心包，
> 体积可控；要完整代码把 `community/openai-agents-python` 当正常 git 仓库 `git sparse-checkout disable` 即可。

---

## 四、多任务并发 Agent（线程池 / asyncio）

场景：用户一次性丢来多个**互相独立**的任务，希望它们并发跑而不是排队。
做法：给每个任务分配一个 agent worker，用「池」统一控制并发数量。

### 业界主流思路（复用来源）

| 来源 | 做法 |
|------|------|
| Python 标准库 `concurrent.futures` | `ThreadPoolExecutor` 线程池，`submit` + `as_completed` 收集结果 |
| OpenAI Agents SDK | 官方推荐 `asyncio.gather` 并行跑多个独立 agent |
| Microsoft Agent Framework | concurrent orchestration：多 agent 并行 + 结果聚合 |
| AutoGen | orchestrator + worker agents + message queue |

**关键点：LLM 调用是网络 I/O 密集型**（大部分时间在等 API 返回），所以线程池 / asyncio 都能显著提速，不需要多进程。

### 两个参考实现

- `concurrent_agent.py`（**线程池版**，主推）
  - `ConcurrentAgentPool`：`ThreadPoolExecutor(max_workers=N)` 控制同时最多跑几个 agent（天然限流）
  - `submit` 全部任务 → `as_completed` 谁先完成先收谁
  - 每个任务独立 try/except，**故障隔离**（一个失败不拖垮其它）
  - 支持单任务超时、进度回调、线程安全日志

- `concurrent_agent_async.py`（**asyncio 版**，对照）
  - `asyncio.Semaphore(N)` 限流 + `asyncio.gather` 收集
  - 真实模式用 `AsyncOpenAI` 异步客户端

### 线程池 vs asyncio 怎么选

| | 线程池 ThreadPoolExecutor | asyncio |
|---|---|---|
| 改造成本 | 低，同步代码直接丢进去 | 高，需全链路 async |
| 并发规模 | 几十个合适 | 轻松成百上千 |
| 调试 | 直观 | 事件循环稍绕 |
| 建议 | **默认首选** | 超大规模并发时用 |

### 运行（演示模式，无需 API key）

```bash
python concurrent_agent.py --demo
python concurrent_agent_async.py --demo
```

演示会跑 8 个模拟任务、池大小 4，可直观看到并发调度、补位、故障隔离，以及相对串行的加速比（实测约 3~4x）。
真实模式配好 `OPENAI_API_KEY` / `OPENAI_BASE_URL` / `OPENAI_MODEL` 后直接 `python concurrent_agent.py` 即可。

> 注：`community/` 与 `reused/`、`my_agent_loop.py` 已加入主仓库 `.gitignore`，不会污染主工程。

两份都用 **OpenAI 兼容接口**，可对接 OpenAI / DeepSeek / 通义千问(DashScope) / OpenRouter 等。

---

## 四、快速运行

```bash
cd agent-loop-reference
pip install -r requirements.txt

# 配置环境变量（三选一，示例用通义千问，与本项目后端一致）
set OPENAI_API_KEY=sk-xxxx
set OPENAI_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
set OPENAI_MODEL=qwen-plus

# 跑复用版（会真的执行 shell 命令，谨慎）
python reused/mini_agent.py "列出当前目录的文件"

# 跑我写的 tool-calling 版（安全，只用内置只读工具）
python my_agent_loop.py "帮我算一下 (23*17)+9，再读一下 README.md 的前几行"
```

> 安全提示：`reused/mini_agent.py` 会在本机执行任意 shell 命令，仅用于学习，勿在生产/不受信环境运行。

---

## 五、参考链接

- mini-swe-agent 教程：https://minimal-agent.com/
- OpenAI Agents SDK：https://openai.github.io/openai-agents-python/
- minimal-agent：https://github.com/Antropath/minimal-agent
- mini_agent：https://github.com/sergenes/mini_agent
