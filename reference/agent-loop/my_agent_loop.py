"""
我手写的极简 Agent Loop（原生 Tool-Calling 派）
================================================

和 reused/mini_agent.py 的区别：
  - mini_agent.py：模型把动作写在文本里（```bash），程序正则解析 —— "文本解析派"
  - 本文件：模型通过 API 的 tools/function_call 结构化字段返回工具调用 —— "工具调用派"

这份代码的目的就是把「Agent Loop 的最小闭环」讲清楚：
  思考(LLM) -> 决定调用哪个工具 -> 执行工具 -> 结果回传 -> 再思考 ... 直到不再调用工具

内置 3 个安全的只读示例工具：
  1. calculator   计算数学表达式
  2. read_file    读取文件内容（前 N 行）
  3. list_dir     列出目录内容

运行：
  set OPENAI_API_KEY=...
  set OPENAI_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
  set OPENAI_MODEL=qwen-plus
  python my_agent_loop.py "帮我算一下 (23*17)+9，再看看 README.md 前 10 行"
"""

import os
import sys
import json
import ast
import operator

from openai import OpenAI

MODEL = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
client = OpenAI()  # 读取 OPENAI_API_KEY / OPENAI_BASE_URL


# ---------------------------------------------------------------------------
# 1) 工具实现：每个工具就是一个普通 Python 函数
# ---------------------------------------------------------------------------

# 安全的表达式求值（只允许 + - * / % ** 和括号，不用 eval 防注入）
_OPS = {
    ast.Add: operator.add, ast.Sub: operator.sub,
    ast.Mult: operator.mul, ast.Div: operator.truediv,
    ast.Mod: operator.mod, ast.Pow: operator.pow,
    ast.USub: operator.neg, ast.UAdd: operator.pos,
}


def _safe_eval(node):
    if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
        return node.value
    if isinstance(node, ast.BinOp) and type(node.op) in _OPS:
        return _OPS[type(node.op)](_safe_eval(node.left), _safe_eval(node.right))
    if isinstance(node, ast.UnaryOp) and type(node.op) in _OPS:
        return _OPS[type(node.op)](_safe_eval(node.operand))
    raise ValueError("表达式包含不支持的运算")


def calculator(expression: str) -> str:
    tree = ast.parse(expression, mode="eval")
    return str(_safe_eval(tree.body))


def read_file(path: str, max_lines: int = 20) -> str:
    if not os.path.isfile(path):
        return f"错误：文件不存在 {path}"
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        lines = [next(f, None) for _ in range(max_lines)]
    lines = [ln for ln in lines if ln is not None]
    return "".join(lines) or "(空文件)"


def list_dir(path: str = ".") -> str:
    if not os.path.isdir(path):
        return f"错误：目录不存在 {path}"
    entries = sorted(os.listdir(path))
    return "\n".join(entries) if entries else "(空目录)"


# 名字 -> 函数 的注册表
TOOL_IMPLS = {
    "calculator": calculator,
    "read_file": read_file,
    "list_dir": list_dir,
}

# ---------------------------------------------------------------------------
# 2) 工具 schema：告诉模型每个工具的名字/用途/参数（OpenAI function calling 格式）
# ---------------------------------------------------------------------------
TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "calculator",
            "description": "计算一个数学表达式，支持 + - * / % ** 和括号。",
            "parameters": {
                "type": "object",
                "properties": {
                    "expression": {"type": "string", "description": "如 (23*17)+9"}
                },
                "required": ["expression"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "读取文本文件的前若干行内容。",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径"},
                    "max_lines": {"type": "integer", "description": "最多读多少行，默认 20"},
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "list_dir",
            "description": "列出某个目录下的文件和子目录。",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "目录路径，默认当前目录"}
                },
            },
        },
    },
]

SYSTEM_PROMPT = (
    "你是一个会使用工具的助手。需要时调用提供的工具来获取信息或完成计算，"
    "拿到工具结果后继续推理。当任务完成、不再需要工具时，直接用自然语言给出最终答案。"
)


# ---------------------------------------------------------------------------
# 3) 核心：Agent Loop
# ---------------------------------------------------------------------------
def run(task: str, max_steps: int = 10):
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": task},
    ]

    for step in range(1, max_steps + 1):
        # (1) 调模型，并把可用工具告诉它
        resp = client.chat.completions.create(
            model=MODEL,
            messages=messages,
            tools=TOOLS,
            tool_choice="auto",
        )
        msg = resp.choices[0].message

        # 把模型这轮的回复原样加入历史（可能包含 tool_calls）
        messages.append(msg.model_dump(exclude_none=True))

        # (2) 模型没有要调用工具 => 任务结束，输出最终答案
        if not msg.tool_calls:
            print(f"\n=== 最终答案（第 {step} 步）===\n{msg.content}")
            return msg.content

        # (3) 逐个执行模型请求的工具调用（可能一次调多个）
        for call in msg.tool_calls:
            name = call.function.name
            try:
                args = json.loads(call.function.arguments or "{}")
            except json.JSONDecodeError:
                args = {}

            print(f"\n[第 {step} 步] 调用工具 {name}({args})")
            fn = TOOL_IMPLS.get(name)
            if fn is None:
                result = f"错误：未知工具 {name}"
            else:
                try:
                    result = fn(**args)
                except Exception as e:  # 工具报错也回传给模型，让它自己决定怎么办
                    result = f"工具执行出错：{e}"
            print(f"   -> 结果: {str(result)[:200]}")

            # (4) 把工具结果作为 role=tool 消息回传，进入下一轮
            messages.append({
                "role": "tool",
                "tool_call_id": call.id,
                "content": str(result),
            })

    print("\n=== 达到最大步数上限，停止 ===")


if __name__ == "__main__":
    task = " ".join(sys.argv[1:]) or "帮我算一下 (23*17)+9，再列出当前目录有哪些文件。"
    run(task)
