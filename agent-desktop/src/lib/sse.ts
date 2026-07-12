/**
 * SSE（Server-Sent Events）流解析工具
 *
 * 从 ReadableStream 中提取 OpenAI 兼容的 SSE 流数据，
 * 统一浏览器端的解析逻辑。
 *
 * 用法：
 *   await parseSSEStream(reader, {
 *     onToken: (token) => console.log(token),
 *     onDone: () => console.log("done"),
 *     signal: abortController.signal,
 *   });
 */

export interface SSEDelta {
  choices?: Array<{ delta?: { content?: string; reasoning_content?: string } }>;
}

export interface SSEParseCallbacks {
  onToken: (token: string) => void;
  /** DeepSeek 思考链增量 （reasoning_content 字段） */
  onThinking?: (token: string) => void;
  onDone?: () => void;
  onLine?: (line: string) => void;
  signal?: AbortSignal;
}

/**
 * 解析 OpenAI 兼容的 SSE 流
 *
 * 流格式：
 *   data: {"choices":[{"delta":{"content":"你好"}}]}
 *   data: {"choices":[{"delta":{"content":"，"}}]}
 *   ...
 *   data: [DONE]
 */
export async function parseSSEStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  callbacks: SSEParseCallbacks,
): Promise<void> {
  const { onToken, onDone, signal } = callbacks;
  const decoder = new TextDecoder();
  let buffer = "";

  try {
    while (true) {
      // 检查取消信号
      if (signal?.aborted) break;

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      while (buffer.includes("\n")) {
        const idx = buffer.indexOf("\n");
        const line = buffer.slice(0, idx).trim();
        buffer = buffer.slice(idx + 1);

        if (!line || !line.startsWith("data: ")) continue;

        const data = line.slice(6);
        if (data === "[DONE]") {
          onDone?.();
          return;
        }

        try {
          const parsed: SSEDelta = JSON.parse(data);
          const delta = parsed.choices?.[0]?.delta;
          if (!delta) continue;
          // reasoning_content 和 content 互斥，优先检测思考链
          if (delta.reasoning_content && callbacks.onThinking) {
            callbacks.onThinking(delta.reasoning_content);
            continue;  // 思考链 token 不计入 content
          }
          const token = delta.content;
          if (token) {
            onToken(token);
          }
        } catch {
          // 跳过无法解析的行
        }
      }
    }
  } finally {
    // 流结束时释放 reader
    try {
      reader.releaseLock();
    } catch {
      // reader 可能已被释放
    }
  }

  onDone?.();
}
