#!/usr/bin/env node
/**
 * Tavily MCP Server（内置，零依赖）
 *
 * 通过 stdio + JSON-RPC 2.0 与桌面端（Rust MCP 客户端）通信。
 * 提供基于 Tavily API 的联网搜索和内容提取工具。
 *
 * 需要环境变量 TAVILY_API_KEY（从 https://tavily.com 获取免费 Key）。
 * 仅依赖 Node.js 内置模块（全局 fetch 需 Node 18+）。
 */

const TAVILY_API_KEY = process.env.TAVILY_API_KEY || "";
const TAVILY_BASE = "https://api.tavily.com";

const TOOLS = [
  {
    name: "tavily_search",
    description:
      "使用 Tavily AI 搜索引擎进行深度联网搜索。返回高相关性的网页标题、链接、摘要和内容片段。适合需要最新信息的查询，结果质量高于传统搜索引擎。",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "搜索查询关键词" },
        search_depth: {
          type: "string",
          enum: ["basic", "advanced"],
          description: "搜索深度：basic 快速搜索（默认），advanced 深度搜索（更全面但更慢）",
        },
        max_results: {
          type: "number",
          description: "返回结果数量，默认 5，最大 10",
        },
        include_domains: {
          type: "array",
          items: { type: "string" },
          description: "限定搜索的域名列表，如 ['wikipedia.org']",
        },
        exclude_domains: {
          type: "array",
          items: { type: "string" },
          description: "排除的域名列表",
        },
      },
      required: ["query"],
    },
  },
  {
    name: "tavily_extract",
    description:
      "使用 Tavily Extract 从指定 URL 列表中提取干净、结构化的网页正文内容。适合阅读搜索结果中感兴趣的页面。",
    inputSchema: {
      type: "object",
      properties: {
        urls: {
          type: "array",
          items: { type: "string" },
          description: "要提取内容的 URL 列表（最多 5 个）",
        },
        extract_depth: {
          type: "string",
          enum: ["basic", "advanced"],
          description: "提取深度：basic 基础提取（默认），advanced 高级提取",
        },
      },
      required: ["urls"],
    },
  },
];

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function makeResult(text, isError = false) {
  return { content: [{ type: "text", text }], isError };
}

// --------------- Tavily API 调用 ---------------

async function tavilySearch(params) {
  if (!TAVILY_API_KEY) {
    return "错误：未设置 TAVILY_API_KEY 环境变量。请在设置中配置 Tavily API Key（可从 https://tavily.com 免费获取）。";
  }

  const body = {
    api_key: TAVILY_API_KEY,
    query: params.query,
    search_depth: params.search_depth || "basic",
    max_results: Math.min(Math.max((params.max_results || 5) | 0, 1), 10),
  };

  if (params.include_domains?.length) body.include_domains = params.include_domains;
  if (params.exclude_domains?.length) body.exclude_domains = params.exclude_domains;

  const res = await fetch(`${TAVILY_BASE}/search`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) {
    const errText = await res.text();
    return `Tavily 搜索请求失败 (HTTP ${res.status}): ${errText}`;
  }

  const data = await res.json();
  if (!data.results || data.results.length === 0) {
    return "未找到相关结果。";
  }

  let output = `搜索查询: ${data.query || params.query}\n`;
  output += `响应时间: ${data.response_time || "N/A"}s\n\n`;

  data.results.forEach((r, i) => {
    output += `${i + 1}. ${r.title || "无标题"}\n`;
    output += `   URL: ${r.url}\n`;
    if (r.content) {
      output += `   内容: ${r.content.slice(0, 300)}${r.content.length > 300 ? "..." : ""}\n`;
    }
    if (r.raw_content && (!r.content || r.raw_content !== r.content)) {
      output += `   原文: ${r.raw_content.slice(0, 200)}...\n`;
    }
    output += `   相关度: ${r.score != null ? r.score.toFixed(2) : "N/A"}\n`;
    output += "\n";
  });

  // 如果有 AI 生成的答案，追加
  if (data.answer) {
    output += `--- AI 综合答案 ---\n${data.answer}\n`;
  }

  return output.trim();
}

async function tavilyExtract(params) {
  if (!TAVILY_API_KEY) {
    return "错误：未设置 TAVILY_API_KEY 环境变量。请在设置中配置 Tavily API Key（可从 https://tavily.com 免费获取）。";
  }

  const urls = (params.urls || []).slice(0, 5);
  if (urls.length === 0) {
    return "错误：请提供至少一个 URL。";
  }

  const body = {
    api_key: TAVILY_API_KEY,
    urls,
    extract_depth: params.extract_depth || "basic",
  };

  const res = await fetch(`${TAVILY_BASE}/extract`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) {
    const errText = await res.text();
    return `Tavily 提取请求失败 (HTTP ${res.status}): ${errText}`;
  }

  const data = await res.json();

  if (data.failed_results?.length) {
    const failedUrls = data.failed_results.map((f) => f.url).join(", ");
    return `部分 URL 提取失败: ${failedUrls}\n\n${data.results?.map((r) => `--- ${r.url} ---\n${r.raw_content || "（无内容）"}`).join("\n\n") || ""}`.trim();
  }

  if (!data.results || data.results.length === 0) {
    return "未能从提供的 URL 中提取到内容。";
  }

  let output = "";
  data.results.forEach((r, i) => {
    output += `--- ${r.url} ---\n`;
    output += `${r.raw_content || "（无内容）"}\n`;
    if (i < data.results.length - 1) output += "\n";
  });

  return output.trim();
}

// --------------- JSON-RPC 处理 ---------------

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
        serverInfo: { name: "tavily-tools", version: "0.2.0" },
      },
    });
  } else if (msg.method === "tools/list") {
    send({ jsonrpc: "2.0", id: msg.id, result: { tools: TOOLS } });
  } else if (msg.method === "tools/call") {
    const name = msg.params?.name;
    const args = msg.params?.arguments || {};
    try {
      let out;
      if (name === "tavily_search") out = await tavilySearch(args);
      else if (name === "tavily_extract") out = await tavilyExtract(args);
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
