"""
my_agent_loop.py —— 逐行讲解注释版（教学用，原文件保持不变）
============================================================

本文件 = 「Agent Loop 的最小闭环」参考实现（原生 Tool-Calling 派）。

完整闭环一句话：
    思考(LLM) -> 决定调用哪个工具 -> 执行工具 -> 结果回传 -> 再思考
    …… 如此循环，直到模型不再请求工具，就输出最终答案。

为了让你看懂，下面每一行（或每一段）都配了中文讲解。
真实运行需要 pip install openai 并配置 OPENAI_API_KEY / OPENAI_BASE_URL / OPENAI_MODEL。
"""


# ============================ 第 0 步：导入 & 初始化 ============================
import os
import sys
import json
import ast            # 抽象语法树，用来"安全地"解析数学表达式（后面 calculator 用）
import operator       # 运算符对应的函数，+ 对应 operator.add 等

from openai import OpenAI   # OpenAI 官方 SDK；因它兼容 OpenAI 接口，
                            # 所以通义千问 / DeepSeek / OpenRouter 都能用（改 BASE_URL 即可）

# 模型名：优先取环境变量 OPENAI_MODEL，没配就用一个默认名
MODEL = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")

# 创建一个客户端对象。它会自动读取两个环境变量：
#   OPENAI_API_KEY  —— 你的 API 密钥
#   OPENAI_BASE_URL —— 接口地址（默认 api.openai.com，换成别的厂商就改这里）
client = OpenAI()


# ============================ 第 1 步：定义「工具」============================
# 核心思想：每个工具就是一个普通的 Python 函数。
# 模型不会真的"执行"什么，它只会告诉我们"我想调用 calculator，参数是 xxx"；
# 真正干活的是下面这些函数，由我们的代码去调用。

# --- calculator：一个"安全"的计算器 -------------------------------------------
# 为什么不直接用 eval()？因为 eval 会执行任意代码，等于把电脑交给模型，
# 极其危险。所以我们用 ast 把表达式解析成"语法树"，只允许 + - * / % ** 和括号。
_OPS = {
    ast.Add: operator.add,    # 加
    ast.Sub: operator.sub,    # 减
    ast.Mult: operator.mul,   # 乘
    ast.Div: operator.truediv,# 除
    ast.Mod: operator.mod,    # 取模 %
    ast.Pow: operator.pow,    # 幂 **
    ast.USub: operator.neg,  # 一元负号，如 -3
    ast.UAdd: operator.pos,   # 一元正号，如 +3
}

def _safe_eval(node):
    """递归遍历语法树，算出表达式的值；遇到不允许的运算符就报错。"""
    if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
        return node.value                      # 叶子：数字本身
    if isinstance(node, ast.BinOp) and type(node.op) in _OPS:
        # 二元运算（如 a + b）：左右分别求值，再用对应运算符合并
        return _OPS[type(node.op)](_safe_eval(node.left), _safe_eval(node.right))
    if isinstance(node, ast.UnaryOp) and type(node.op) in _OPS:
        # 一元运算（如 -a）
        return _OPS[type(node.op)](_safe_eval(node.operand))
    raise ValueError("表达式包含不支持的运算")

def calculator(expression: str) -> str:
    """计算一个数学表达式，返回字符串形式的结果。"""
    tree = ast.parse(expression, mode="eval")   # 把字符串解析成语法树
    return str(_safe_eval(tree.body))            # 计算结果并转成字符串

# --- read_file：读取文本文件前 N 行（只读，安全）-----------------------------------
def read_file(path: str, max_lines: int = 20) -> str:
    if not os.path.isfile(path):
        return f"错误：文件不存在 {path}"          # 友好报错，而不是崩溃
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        # 只读前 max_lines 行，避免读超大文件把内存撑爆
        lines = [next(f, None) for _ in range(max_lines)]
    lines = [ln for ln in lines if ln is not None]  # 去掉读不到的 None
    return "".join(lines) or "(空文件)"

# --- list_dir：列出目录内容（只读，安全）---------------------------------------
def list_dir(path: str = ".") -> str:
    if not os.path.isdir(path):
        return f"错误：目录不存在 {path}"
    entries = sorted(os.listdir(path))
    return "\n".join(entries) if entries else "(空目录)"

# 把"工具名字 -> 真实函数"做一张映射表，后面执行时靠名字查表调用。
TOOL_IMPLS = {
    "calculator": calculator,
    "read_file": read_file,
    "list_dir": list_dir,
}


# ============================ 第 2 步：给模型看的「工具说明书」============================
# 模型怎么知道有哪些工具可用、该怎么传参？靠下面这份 TOOLS 列表。
# 这是 OpenAI function calling 约定的格式，相当于给模型的"菜单 + 点单说明"。
TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "calculator",
            # description 极其重要：模型靠这段描述来判断"什么时候该用这个工具"
            "description": "计算一个数学表达式，支持 + - * / % ** 和括号。",
            "parameters": {                       # 用 JSON Schema 描述参数
                "type": "object",
                "properties": {
                    "expression": {"type": "string", "description": "如 (23*17)+9"}
                },
                "required": ["expression"],       # 必填字段
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

# 系统提示词：给模型的"人设 + 行为准则"。这里告诉它"你会用工具、用完继续想"。
SYSTEM_PROMPT = (
    "你是一个会使用工具的助手。需要时调用提供的工具来获取信息或完成计算，"
    "拿到工具结果后继续推理。当任务完成、不再需要工具时，直接用自然语言给出最终答案。"
)


# ============================ 第 3 步：Agent Loop 本体 ============================
def run(task: str, max_steps: int = 10):
    """
    跑一个完整任务。参数：
      task       : 用户的任务描述
      max_steps  : 最多循环几轮（防止模型死循环，兜底）
    """

    # (A) 初始化「对话历史」messages。
    #     这就是模型的"记忆"——每一轮模型看到的全部上下文都在这里。
    #     随着时间的推移，我们会不断往里 append 新内容（模型的话、工具的结果）。
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},  # 系统设定（不会显示给用户）
        {"role": "user",   "content": task},             # 用户任务
    ]

    # (B) 主循环：这就是 Agent Loop 的"圈"。
    #     range(1, max_steps+1) 意味着最多转 max_steps 圈，到数就停（兜底保护）。
    for step in range(1, max_steps + 1):

        # ---- (1) 思考：调用大模型，并把"可用工具"一并告诉它 ----
        resp = client.chat.completions.create(
            model=MODEL,
            messages=messages,     # 把到目前为止的全部对话历史都发给模型
            tools=TOOLS,           # 告诉模型：你有这 3 个工具可以用
            tool_choice="auto",    # auto = 让模型自己决定"调不调、调哪个"
        )
        msg = resp.choices[0].message    # 取模型这一轮的回复对象

        # ---- 把模型这轮的回复原样记进历史 ----
        # 注意：msg 里可能藏着 tool_calls（"我要调工具"的结构化信息），
        # 一定要原样保存，否则下一轮模型就"忘了"自己刚才想干嘛。
        messages.append(msg.model_dump(exclude_none=True))

        # ---- (2) 判断：模型这轮有没有要调工具？ ----
        # 如果没有 tool_calls，说明模型认为任务已经完成，直接给了最终答案。
        if not msg.tool_calls:
            print(f"\n=== 最终答案（第 {step} 步）===\n{msg.content}")
            return msg.content     # 跳出循环（任务结束）

        # ---- (3) 动手：模型要调工具，我们就替它去执行 ----
        # 模型一次可能请求调多个工具，所以用 for 逐个处理。
        for call in msg.tool_calls:
            name = call.function.name                 # 比如 "calculator"
            # 参数是 JSON 字符串，需要解析成 Python 字典
            try:
                args = json.loads(call.function.arguments or "{}")
            except json.JSONDecodeError:
                args = {}                            # 解析失败就当无参数，别让程序崩

            print(f"\n[第 {step} 步] 调用工具 {name}({args})")

            # 用名字去映射表里查真正的函数
            fn = TOOL_IMPLS.get(name)
            if fn is None:
                # 模型有时候会"瞎编"一个不存在的工具名，做个兜底
                result = f"错误：未知工具 {name}"
            else:
                try:
                    # 真正执行工具！**这里是模型唯一能"影响现实"的地方。**
                    result = fn(**args)
                except Exception as e:
                    # 关键设计：工具报错也别崩，把错误信息回传给模型，
                    # 让它自己看到错误、决定下一步（比如换个参数重试）。
                    result = f"工具执行出错：{e}"
            print(f"   -> 结果: {str(result)[:200]}")

            # ---- (4) 回传：把工具结果塞回对话历史 ----
            # role="tool" 是 OpenAI 约定的"工具结果"消息类型；
            # tool_call_id 必须和前面的调用对上，模型才知道"这是哪次调用的结果"。
            messages.append({
                "role": "tool",
                "tool_call_id": call.id,
                "content": str(result),
            })
        # 一轮里所有工具都跑完、结果都塞回去后，
        # 循环回到顶部 (1)，模型带着"新得到的工具结果"再思考一轮……
        # ……这就是"Loop"：思考 → 调工具 → 看结果 → 再思考，直到不再调工具。

    # 如果转满了 max_steps 圈还没结束（比如模型一直在调工具），就强制停。
    print("\n=== 达到最大步数上限，停止 ===")


if __name__ == "__main__":
    # 从命令行取任务；没给就用一句默认示例
    task = " ".join(sys.argv[1:]) or "帮我算一下 (23*17)+9，再列出当前目录有哪些文件。"
    run(task)
