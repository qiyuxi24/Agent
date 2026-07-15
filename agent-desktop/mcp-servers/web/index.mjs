#!/usr/bin/env node
/**
 * Web Tools MCP Server（内置，零依赖、无需 API Key）
 *
 * 通过 stdio + JSON-RPC 2.0 与桌面端（Rust MCP 客户端）通信。
 * 提供两个工具：
 *   - web_search：联网搜索（基于 DuckDuckGo HTML，无需 Key）
 *   - fetch_url ：爬取并读取任意网页正文（去噪为纯文本）
 *
 * 仅依赖 Node.js 内置模块（全局 fetch 需 Node 18+）。
 */

const TOOLS = [
  {
    name: "web_search",
    description:
      "联网搜索：输入查询关键词，返回相关网页的标题、链接与摘要。无需 API Key（基于 DuckDuckGo）。",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "搜索关键词" },
        limit: { type: "number", description: "返回结果数量，默认 5，最大 10" },
      },
      required: ["query"],
    },
  },
  {
    name: "fetch_url",
    description:
      "爬取并读取网页内容：输入 URL，返回其正文纯文本（去除脚本/样式噪声），可用于阅读搜索结果或任意网页。",
    inputSchema: {
      type: "object",
      properties: {
        url: { type: "string", description: "要抓取的网页 URL" },
      },
      required: ["url"],
    },
  },
];

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function makeResult(text, isError = false) {
  return { content: [{ type: "text", text }], isError };
}

function stripTags(s) {
  return s.replace(/<[^>]+>/g, " ");
}

function decodeHTML(s) {
  return s
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;|&#x27;/g, "'")
    .replace(/&nbsp;/g, " ");
}

/**
 * 带超时的 fetch 封装
 * @param {string} url
 * @param {RequestInit} options
 * @param {number} timeoutMs 超时毫秒数
 */
async function fetchWithTimeout(url, options = {}, timeoutMs = 30000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, { ...options, signal: controller.signal });
    return res;
  } catch (e) {
    if (e.name === "AbortError") {
      throw new Error(`请求超时 (${timeoutMs / 1000}s)`);
    }
    throw e;
  } finally {
    clearTimeout(timer);
  }
}

async function webSearch(query, limit = 5) {
  const max = Math.min(Math.max(limit | 0, 1), 10);
  const url = `https://html.duckduckgo.com/html/?q=${encodeURIComponent(query)}`;
  const res = await fetchWithTimeout(url, {
    headers: { "User-Agent": "Mozilla/5.0 (compatible; Votek/0.3; +https://github.com/346379/Agent)" },
  }, 15000);

  if (!res.ok) {
    throw new Error(`DuckDuckGo 搜索失败: HTTP ${res.status}`);
  }

  const html = await res.text();

  const results = [];
  const re =
    /<a[^>]+class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>[\s\S]*?<a[^>]+class="result__snippet"[^>]*>([\s\S]*?)<\/a>/g;
  let m;
  while ((m = re.exec(html)) !== null && results.length < max) {
    const href = decodeHTML(m[1]);
    const title = decodeHTML(stripTags(m[2])).replace(/\s+/g, " ").trim();
    const snippet = decodeHTML(stripTags(m[3])).replace(/\s+/g, " ").trim();
    if (title) results.push({ title, url: href, snippet });
  }

  if (results.length === 0) {
    return "未找到相关结果，或搜索服务暂时不可用。";
  }
  return results
    .map((r, i) => `${i + 1}. ${r.title}\n   ${r.url}\n   ${r.snippet}`)
    .join("\n\n");
}

async function fetchUrl(url) {
  // 验证 URL 格式
  let targetUrl;
  try {
    targetUrl = new URL(url);
    if (!["http:", "https:"].includes(targetUrl.protocol)) {
      return `不支持的协议: ${targetUrl.protocol}（仅支持 http/https）`;
    }
  } catch {
    return `无效的 URL 格式: "${url}"。请提供完整的 http:// 或 https:// 链接。`;
  }

  const res = await fetchWithTimeout(url, {
    headers: {
      "User-Agent": "Mozilla/5.0 (compatible; Votek/0.3; +https://github.com/346379/Agent)",
      "Accept": "text/html, text/plain, application/json, application/xml, */*",
      "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
    },
    redirect: "follow",
  }, 30000);

  if (!res.ok) {
    return `HTTP ${res.status} ${res.statusText}: 无法获取该 URL（可能不存在、需要登录或被屏蔽）`;
  }

  const contentType = res.headers.get("content-type") || "";
  if (!contentType.includes("html") && !contentType.includes("text")
      && !contentType.includes("json") && !contentType.includes("xml")) {
    const contentLength = res.headers.get("content-length") || "未知";
    return `该 URL 返回的内容类型为 ${contentType}（大小 ${contentLength} 字节），无法直接以文本形式读取。`;
  }

  const html = await res.text();
  let text = decodeHTML(stripTags(html));
  text = text.replace(/[ \t]+/g, " ").replace(/\n{3,}/g, "\n\n").trim();
  const max = 8000;
  if (text.length > max) text = text.slice(0, max) + "\n...（内容已截断，原文更长）";

  if (!text.trim()) {
    return "页面内容为空（可能是需要 JavaScript 渲染的单页应用，或重定向到了空白页）。";
  }

  return text;
}

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let idx;
  while ((idx = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, idx).trim();
    buffer = buffer.slice(idx + 1);
    if (line) handle(line);
  }
});

async function handle(line) {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }

  // 通知类（无 id）直接忽略
  if (msg.id === undefined && typeof msg.method === "string" && msg.method.startsWith("notifications/")) {
    return;
  }

  if (msg.method === "initialize") {
    send({
      jsonrpc: "2.0",
      id: msg.id,
      result: {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "web-tools", version: "0.2.0" },
      },
    });
  } else if (msg.method === "tools/list") {
    send({ jsonrpc: "2.0", id: msg.id, result: { tools: TOOLS } });
  } else if (msg.method === "tools/call") {
    const name = msg.params?.name;
    const args = msg.params?.arguments || {};
    try {
      let out;
      if (name === "web_search") out = await webSearch(args.query, args.limit || 5);
      else if (name === "fetch_url") out = await fetchUrl(args.url);
      else out = `未知工具: ${name}`;
      send({ jsonrpc: "2.0", id: msg.id, result: makeResult(out) });
    } catch (e) {
      send({
        jsonrpc: "2.0",
        id: msg.id,
        result: makeResult(`工具执行错误: ${e?.message || e}`, true),
      });
    }
  }
}

// 避免在管道关闭时报错退出
process.stdin.on("error", () => {});
