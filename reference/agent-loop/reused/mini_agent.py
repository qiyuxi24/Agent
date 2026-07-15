"""
复用他人成果：mini-swe-agent 蓝图的极简 Agent Loop
====================================================

来源 / 致谢：
  - swe-agent/mini-swe-agent 教程 https://minimal-agent.com/
    (Authors: Kilian Lieret, Carlos Jimenez, John Yang, Ofir Press)

范式：文本解析派
  模型把要执行的动作写在回复里的 ```bash 代码块中，
  程序用正则提取并在本地 shell 执行，把输出回传给模型，如此循环。

这是「复刻 + 少量健壮性增强」版本，约 90 行，方便直接对照原教程理解。

⚠️ 安全警告：本脚本会在你本机执行任意 shell 命令，仅供学习，切勿在生产环境使用。

运行：
  set OPENAI_API_KEY=...
  set OPENAI_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
  set OPENAI_MODEL=qwen-plus
  python reused/mini_agent.py "列出当前目录的文件"
"""

import os
import re
import sys
import subprocess

from openai import OpenAI

MODEL = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
client = OpenAI()  # 自动读取 OPENAI_API_KEY / OPENAI_BASE_URL

SYSTEM_PROMPT = """You are a helpful assistant that solves tasks by running shell commands.

When you want to run a command, output EXACTLY one action wrapped in a bash code block:

```bash
<your command here>
```

Rules:
- Output at most ONE bash code block per reply.
- After you see the command output, decide the next step.
- When the task is fully done, reply with a final summary and DO NOT include any bash code block.
"""

# 避免命令行工具进入交互模式卡住（来自 mini-swe-agent 的 env 设置）
ENV_VARS = {
    "PAGER": "cat",
    "MANPAGER": "cat",
    "GIT_PAGER": "cat",
    "PIP_PROGRESS_BAR": "off",
    "TQDM_DISABLE": "1",
}


class FormatError(RuntimeError):
    """模型没有按 ```bash 格式输出动作时抛出，回传给模型让它自我纠正。"""


def query_lm(messages):
    resp = client.chat.completions.create(model=MODEL, messages=messages)
    return resp.choices[0].message.content or ""


def parse_action(lm_output: str):
    """从模型回复中解析出 bash 动作；没有动作则返回 None（视为任务结束）。"""
    matches = re.findall(r"```(?:bash|shell|sh)?\s*\n(.*?)\n```", lm_output, re.DOTALL)
    if len(matches) == 0:
        return None  # 没有动作 => 结束
    if len(matches) > 1:
        raise FormatError(
            "你的输出包含了多个代码块，请每次只输出恰好一个 ```bash 动作。"
        )
    return matches[0].strip()


def execute_action(command: str) -> str:
    result = subprocess.run(
        command,
        shell=True,
        text=True,
        env=os.environ | ENV_VARS,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=60,
    )
    out = result.stdout or ""
    return out if out.strip() else "(命令执行完成，无输出)"


def run(task: str, max_steps: int = 20):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": task},
    ]
    for step in range(1, max_steps + 1):
        try:
            lm_output = query_lm(messages)
            print(f"\n=== 第 {step} 步 · 模型输出 ===\n{lm_output}")
            messages.append({"role": "assistant", "content": lm_output})

            action = parse_action(lm_output)
            if action is None:
                print("\n=== 任务结束（模型没有再给出动作）===")
                return lm_output

            print(f"\n>>> 执行: {action}")
            output = execute_action(action)
            print(f"<<< 输出:\n{output}")
            messages.append({"role": "user", "content": f"命令输出：\n{output}"})

        except FormatError as e:
            # 已知的格式错误：告诉模型，让它自己改正后继续
            messages.append({"role": "user", "content": str(e)})
        except subprocess.TimeoutExpired:
            messages.append({"role": "user", "content": "上一条命令超时了（>60s），请换个更快/非交互的做法。"})

    print("\n=== 达到最大步数上限，停止 ===")


if __name__ == "__main__":
    task = " ".join(sys.argv[1:]) or "列出当前目录下的文件并统计有多少个。"
    run(task)
