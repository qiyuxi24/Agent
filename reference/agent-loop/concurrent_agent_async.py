"""
多任务并发 Agent 池（asyncio 版，与线程池版对照）
================================================

这是 concurrent_agent.py（线程池版）的「协程」对照版本。
复用的业界思路：OpenAI Agents SDK 官方推荐用 asyncio.gather 并行跑多个独立 agent，
再用 asyncio.Semaphore 控制并发上限（相当于线程池的 max_workers）。

线程池 vs asyncio 怎么选？
  - 线程池（ThreadPoolExecutor）：改造成本低，同步代码直接丢进去就并发；调试直观。
      适合：现有同步 SDK、任务数不多（几十个）。
  - asyncio：单线程事件循环，无线程切换开销，能轻松撑「成百上千」并发 I/O。
      适合：超大规模并发、全链路都是 async SDK（如 openai 的 AsyncOpenAI）。
  两者对 LLM 这种 I/O 密集任务都能显著提速；线程池更易上手，asyncio 更能扛量。

运行：
  python concurrent_agent_async.py            # 演示模式（无需 key）
  # 真实模式需 pip install openai 并配置 OPENAI_API_KEY / OPENAI_BASE_URL / OPENAI_MODEL
"""

from __future__ import annotations

import os
import sys
import time
import random
import asyncio
from dataclasses import dataclass


@dataclass
class AgentTask:
    id: str
    prompt: str


@dataclass
class AgentResult:
    task_id: str
    ok: bool
    output: str = ""
    error: str = ""
    elapsed: float = 0.0


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# --- worker：单个 agent 的协程 --------------------------------------------
async def dummy_worker(task: AgentTask) -> str:
    delay = random.uniform(1.0, 3.0)
    await asyncio.sleep(delay)  # 用 await sleep 模拟异步 I/O 等待
    if "fail" in task.prompt.lower():
        raise RuntimeError("模拟任务失败")
    return f"（模拟）完成：{task.prompt}  用时 {delay:.1f}s"


async def real_worker(task: AgentTask) -> str:
    from openai import AsyncOpenAI  # 异步客户端

    client = AsyncOpenAI()
    model = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
    resp = await client.chat.completions.create(
        model=model,
        messages=[
            {"role": "system", "content": "你是一个高效的助手，简洁作答。"},
            {"role": "user", "content": task.prompt},
        ],
    )
    return resp.choices[0].message.content or ""


# --- Orchestrator：用 Semaphore 限流 + gather 收集 -------------------------
async def run_pool(tasks, worker, max_concurrent: int = 4):
    sem = asyncio.Semaphore(max_concurrent)  # 相当于线程池的 max_workers

    async def guarded(task: AgentTask) -> AgentResult:
        async with sem:  # 超过并发上限的协程会在这里排队等待
            start = time.time()
            log(f"▶ 开始 [{task.id}]: {task.prompt[:40]}")
            try:
                out = await worker(task)
                r = AgentResult(task.id, True, output=out, elapsed=time.time() - start)
                log(f"✔ 完成 [{task.id}] 用时 {r.elapsed:.1f}s")
                return r
            except Exception as e:  # 故障隔离
                r = AgentResult(task.id, False, error=f"{type(e).__name__}: {e}",
                                elapsed=time.time() - start)
                log(f"✗ 失败 [{task.id}]: {r.error}")
                return r

    # return_exceptions=True 让单个协程异常不会中断整个 gather
    return await asyncio.gather(*(guarded(t) for t in tasks), return_exceptions=False)


async def main():
    demo = "--demo" in sys.argv or not os.environ.get("OPENAI_API_KEY")
    worker = dummy_worker if demo else real_worker
    log(f"=== 并发 Agent 池（asyncio）· {'演示' if demo else '真实'}模式 ===")

    prompts = ["整理会议纪要", "翻译文案", "生成周报", "分析数据",
               "这个任务会 fail", "写邮件", "总结论文", "起产品名"]
    tasks = [AgentTask(f"T{i}", p) for i, p in enumerate(prompts, 1)]

    t0 = time.time()
    results = await run_pool(tasks, worker, max_concurrent=4)
    total = time.time() - t0

    ok = sum(1 for r in results if r.ok)
    print("\n" + "=" * 50)
    log(f"全部完成：成功 {ok} / 失败 {len(results) - ok}，总耗时 {total:.1f}s")
    if demo:
        serial = sum(r.elapsed for r in results)
        print(f"若串行约需 {serial:.1f}s，并发实际 {total:.1f}s，加速约 {serial / total:.1f}x")


if __name__ == "__main__":
    asyncio.run(main())
