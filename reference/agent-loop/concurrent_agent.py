"""
多任务并发 Agent 池（线程池版）
================================================

场景：用户一次性丢来多个互相独立的任务，希望它们「并发」跑，而不是排队一个个来。
做法：给每个任务分配一个 Agent worker，用「线程池」统一调度并发数量。

为什么用线程池（ThreadPoolExecutor）而不是多进程？
  - LLM 调用本质是「网络 I/O 等待」，绝大部分时间在等 API 返回，不吃 CPU。
  - I/O 密集 => 线程池最合适（GIL 在 I/O 等待时会释放，多个线程能真正并发等待）。
  - 多进程（ProcessPoolExecutor）适合 CPU 密集，这里用不上，还会增加序列化开销。

复用的业界思路：
  - concurrent.futures.ThreadPoolExecutor          —— Python 官方标准线程池
  - Orchestrator + Worker Pool 模式（AutoGen / MS Agent Framework）
      * Orchestrator（本文件的 ConcurrentAgentPool）负责派发任务、收集结果
      * 每个 Worker 是一个独立 Agent，跑自己的那条 loop
  - max_workers 控制并发上限 —— 天然的「限流」，避免打爆 API 速率限制
  - submit + as_completed —— 谁先跑完先拿谁的结果
  - 每个任务独立 try/except —— 一个任务失败不拖垮其它任务（故障隔离）

运行：
  # 真实模式（需要配置 key）
  set OPENAI_API_KEY=...
  set OPENAI_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
  set OPENAI_MODEL=qwen-plus
  python concurrent_agent.py

  # 演示模式（不配 key 也能跑，用模拟 worker 观察线程池调度）
  python concurrent_agent.py --demo
"""

from __future__ import annotations

import os
import sys
import time
import random
import threading
from dataclasses import dataclass, field
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Callable, Optional


# ---------------------------------------------------------------------------
# 数据结构：任务 & 结果
# ---------------------------------------------------------------------------
@dataclass
class AgentTask:
    """一个待执行的任务。"""
    id: str
    prompt: str
    max_steps: int = 6


@dataclass
class AgentResult:
    """一个任务的执行结果。"""
    task_id: str
    ok: bool
    output: str = ""
    error: str = ""
    elapsed: float = 0.0
    worker: str = ""


# 线程安全的打印锁（多个线程同时 print 会串行乱序）
_print_lock = threading.Lock()


def log(msg: str) -> None:
    with _print_lock:
        print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# ---------------------------------------------------------------------------
# Worker：单个 Agent 的执行逻辑（一个任务 = 一次 Agent loop）
# ---------------------------------------------------------------------------
def real_agent_worker(task: AgentTask) -> str:
    """真实 worker：调用 OpenAI 兼容接口，跑一个简单的单/多轮 loop。

    这里为聚焦「并发调度」，用最简单的单轮问答；
    需要工具调用的完整 loop 可直接复用 my_agent_loop.py 里的 run()。
    """
    from openai import OpenAI  # 延迟导入，--demo 模式无需安装

    client = OpenAI()
    model = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
    resp = client.chat.completions.create(
        model=model,
        messages=[
            {"role": "system", "content": "你是一个高效的助手，简洁作答。"},
            {"role": "user", "content": task.prompt},
        ],
    )
    return resp.choices[0].message.content or ""


def dummy_agent_worker(task: AgentTask) -> str:
    """演示 worker：模拟一个耗时 1~3 秒的任务，用来观察线程池并发效果。"""
    delay = random.uniform(1.0, 3.0)
    time.sleep(delay)
    # 模拟偶发失败，验证故障隔离
    if "fail" in task.prompt.lower():
        raise RuntimeError("模拟任务失败")
    return f"（模拟）完成任务：{task.prompt}  用时 {delay:.1f}s"


# ---------------------------------------------------------------------------
# Orchestrator：并发 Agent 池
# ---------------------------------------------------------------------------
class ConcurrentAgentPool:
    """用线程池并发执行多个 Agent 任务。

    参数：
      worker       : 单个任务的执行函数 (AgentTask) -> str
      max_workers  : 线程池大小（同时最多跑几个 agent）—— 天然限流
      task_timeout : 单个任务超时秒数（None 表示不限）
      on_progress  : 每完成一个任务的回调 (AgentResult) -> None
    """

    def __init__(
        self,
        worker: Callable[[AgentTask], str],
        max_workers: int = 4,
        task_timeout: Optional[float] = None,
        on_progress: Optional[Callable[[AgentResult], None]] = None,
    ):
        self.worker = worker
        self.max_workers = max_workers
        self.task_timeout = task_timeout
        self.on_progress = on_progress

    def _run_one(self, task: AgentTask) -> AgentResult:
        """在某个线程里执行单个任务，把成功/异常都封装成 AgentResult（故障隔离）。"""
        worker_name = threading.current_thread().name
        start = time.time()
        log(f"▶ 开始 [{task.id}] on {worker_name}: {task.prompt[:40]}")
        try:
            output = self.worker(task)
            result = AgentResult(
                task_id=task.id, ok=True, output=output,
                elapsed=time.time() - start, worker=worker_name,
            )
            log(f"✔ 完成 [{task.id}] 用时 {result.elapsed:.1f}s")
            return result
        except Exception as e:  # 一个任务炸了不影响别的
            result = AgentResult(
                task_id=task.id, ok=False, error=f"{type(e).__name__}: {e}",
                elapsed=time.time() - start, worker=worker_name,
            )
            log(f"✗ 失败 [{task.id}]: {result.error}")
            return result

    def run(self, tasks: list[AgentTask]) -> list[AgentResult]:
        """提交所有任务，并发执行，返回全部结果（按完成先后顺序）。"""
        results: list[AgentResult] = []
        log(f"提交 {len(tasks)} 个任务，线程池大小 = {self.max_workers}")

        with ThreadPoolExecutor(
            max_workers=self.max_workers, thread_name_prefix="agent"
        ) as pool:
            # 1) 把每个任务 submit 进池子，拿到 future -> task 的映射
            future_to_task = {pool.submit(self._run_one, t): t for t in tasks}

            # 2) 谁先完成先处理谁
            for future in as_completed(future_to_task):
                task = future_to_task[future]
                try:
                    result = future.result(timeout=self.task_timeout)
                except Exception as e:  # 例如超时 TimeoutError
                    result = AgentResult(
                        task_id=task.id, ok=False,
                        error=f"调度层异常: {type(e).__name__}: {e}",
                    )
                results.append(result)
                if self.on_progress:
                    self.on_progress(result)

        return results


# ---------------------------------------------------------------------------
# 演示
# ---------------------------------------------------------------------------
def main():
    demo_mode = "--demo" in sys.argv or not os.environ.get("OPENAI_API_KEY")
    worker = dummy_agent_worker if demo_mode else real_agent_worker

    mode_name = "演示（模拟 worker）" if demo_mode else "真实（调用 LLM）"
    log(f"=== 并发 Agent 池 · {mode_name}模式 ===")

    # 模拟用户一次性丢来的多个独立任务
    if demo_mode:
        tasks = [
            AgentTask(id=f"T{i}", prompt=p)
            for i, p in enumerate([
                "整理会议纪要", "翻译一段文案", "生成周报大纲",
                "分析销售数据", "这个任务会 fail", "写一封邮件",
                "总结论文摘要", "起一个产品名",
            ], 1)
        ]
    else:
        tasks = [
            AgentTask(id="T1", prompt="用一句话解释什么是线程池"),
            AgentTask(id="T2", prompt="给我三个提高专注力的方法"),
            AgentTask(id="T3", prompt="把'今天天气不错'翻译成英文和日文"),
            AgentTask(id="T4", prompt="写一句鼓励人的话"),
        ]

    pool = ConcurrentAgentPool(
        worker=worker,
        max_workers=4,          # 同时最多 4 个 agent，其余排队
        task_timeout=30,        # 单任务超 30s 判定失败
    )

    t0 = time.time()
    results = pool.run(tasks)
    total = time.time() - t0

    # 汇总
    ok = [r for r in results if r.ok]
    fail = [r for r in results if not r.ok]
    print("\n" + "=" * 50)
    log(f"全部完成：成功 {len(ok)} / 失败 {len(fail)}，总耗时 {total:.1f}s")
    print("=" * 50)
    for r in sorted(results, key=lambda x: x.task_id):
        status = "OK " if r.ok else "ERR"
        detail = r.output if r.ok else r.error
        print(f"[{status}] {r.task_id} ({r.elapsed:.1f}s) -> {str(detail)[:60]}")

    # 串行 vs 并发的直观对比（仅演示模式估算）
    if demo_mode:
        serial_est = sum(r.elapsed for r in results)
        print(f"\n若串行执行约需 {serial_est:.1f}s，并发实际 {total:.1f}s，"
              f"加速约 {serial_est / total:.1f}x")


if __name__ == "__main__":
    main()
