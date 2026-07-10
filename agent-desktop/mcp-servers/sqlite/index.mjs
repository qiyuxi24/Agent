#!/usr/bin/env node
/**
 * SQLite MCP Server（内置，零依赖）
 *
 * 通过 stdio + JSON-RPC 2.0 与桌面端（Rust MCP 客户端）通信。
 * 提供本地 SQLite 数据库的读写操作。
 *
 * 使用 better-sqlite3（需 npm install），或 Node 22.5+ built-in node:sqlite。
 */

import * as path from "node:path";
import * as fs from "node:fs";

// ---------- SQLite 驱动加载 ----------

let Database;
let dbModuleName;

try {
  // 方案1: Node 22.5+ built-in（需 npx -y 或直接 node --experimental-sqlite）
  const sqlite = await import("node:sqlite");
  Database = sqlite.DatabaseSync;
  dbModuleName = "node:sqlite (built-in, 实验性)";
} catch {
  // 方案2: better-sqlite3（npm install better-sqlite3）
  try {
    const bsql = await import("better-sqlite3");
    Database = bsql.default;
    dbModuleName = "better-sqlite3";
  } catch {
    Database = null;
    dbModuleName = null;
  }
}

// ---------- 数据库连接 ----------

const DB_PATH = process.env.SQLITE_DB_PATH || "./data/mcp_sqlite.db";
let db = null;

function getDb() {
  if (!Database) {
    throw new Error(
      "SQLite 不可用。请执行以下任一操作：\n" +
      "  1. npm install better-sqlite3\n" +
      "  2. 升级到 Node.js 22.5+，启动时添加 --experimental-sqlite"
    );
  }
  if (!db) {
    const dir = path.dirname(DB_PATH);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    db = new Database(DB_PATH);
    db.exec("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
    process.stderr.write(`[sqlite] 已连接 ${DB_PATH} (${dbModuleName})\n`);
  }
  return db;
}

// ---------- 工具列表 ----------

const TOOLS = [
  {
    name: "sqlite_query",
    description:
      "执行 SELECT 查询并返回结果表格。支持参数化查询 ? 占位符防止 SQL 注入。只读安全操作。",
    inputSchema: {
      type: "object",
      properties: {
        sql: { type: "string", description: "SELECT SQL 查询，如 SELECT * FROM users WHERE age > ?" },
        params: { type: "array", items: {}, description: "查询参数，按顺序替换 ? 占位符" },
      },
      required: ["sql"],
    },
  },
  {
    name: "sqlite_execute",
    description:
      "执行 INSERT/UPDATE/DELETE/CREATE/ALTER/DROP 等写操作。返回影响行数和插入 ID。⚠ 会修改数据库！",
    inputSchema: {
      type: "object",
      properties: {
        sql: { type: "string", description: "SQL 写操作语句" },
        params: { type: "array", items: {}, description: "SQL 参数" },
      },
      required: ["sql"],
    },
  },
  {
    name: "sqlite_schema",
    description:
      "查看数据库结构：列出所有表名及列定义。不指定表名则列出全部表。",
    inputSchema: {
      type: "object",
      properties: {
        table: { type: "string", description: "可选：指定表名查看详情。留空列出所有表" },
      },
    },
  },
];

// ---------- JSON-RPC 辅助 ----------

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function makeResult(text, isError = false) {
  return { content: [{ type: "text", text }], isError };
}

function renderRows(rows) {
  if (!rows || rows.length === 0) return "查询返回 0 行";
  const columns = Object.keys(rows[0]);
  const header = columns.join(" | ");
  const sep = columns.map(() => "---").join(" | ");
  const body = rows.map((r) => columns.map((c) => String(r[c] ?? "NULL")).join(" | ")).join("\n");
  return `${header}\n${sep}\n${body}\n\n共 ${rows.length} 行`;
}

// ---------- 工具实现 ----------

function handleQuery(sql, params = []) {
  const isSelect = /^\s*(SELECT|PRAGMA|EXPLAIN)\b/i.test(sql);
  if (!isSelect) return "错误：sqlite_query 仅支持 SELECT/PRAGMA/EXPLAIN。写操作请用 sqlite_execute。";

  const d = getDb();
  try {
    const stmt = d.prepare(sql);
    const rows = stmt.all(...params);
    return renderRows(rows);
  } catch (e) {
    return `SQL 错误: ${e.message}`;
  }
}

function handleExecute(sql, params = []) {
  const isWrite = /^\s*(INSERT|UPDATE|DELETE|CREATE|ALTER|DROP)\b/i.test(sql);
  if (!isWrite) return "警告：sqlite_execute 用于写操作。SELECT 请用 sqlite_query。";

  const d = getDb();
  try {
    const stmt = d.prepare(sql);
    const result = stmt.run(...params);
    return `✓ 成功。影响行数: ${result.changes}，最后插入 ID: ${result.lastInsertRowid}`;
  } catch (e) {
    return `SQL 错误: ${e.message}`;
  }
}

function handleSchema(table) {
  const d = getDb();
  try {
    if (table) {
      const safe = table.replace(/'/g, "''");
      const rows = d.prepare(`PRAGMA table_info('${safe}')`).all();
      if (rows.length === 0) return `表 "${table}" 不存在。`;
      return `表: ${table}\n${renderRows(rows)}`;
    }

    const tables = d
      .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
      .all()
      .map((r) => r.name);

    if (tables.length === 0) return "数据库中暂无表。";

    let out = `数据库表 (${tables.length}):\n`;
    for (const t of tables) {
      const cols = d.prepare(`PRAGMA table_info('${t.replace(/'/g, "''")}')`).all();
      out += `  ${t} (${cols.map((c) => c.name).join(", ")})\n`;
    }
    return out.trim();
  } catch (e) {
    return `Schema 错误: ${e.message}`;
  }
}

// ---------- 消息路由 ----------

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

function handle(line) {
  let msg;
  try { msg = JSON.parse(line); } catch { return; }

  if (msg.id === undefined && typeof msg.method === "string" && msg.method.startsWith("notifications/")) return;

  if (msg.method === "initialize") {
    send({
      jsonrpc: "2.0", id: msg.id,
      result: {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "sqlite-tools", version: "0.2.0" },
      },
    });
  } else if (msg.method === "tools/list") {
    send({ jsonrpc: "2.0", id: msg.id, result: { tools: TOOLS } });
  } else if (msg.method === "tools/call") {
    const name = msg.params?.name;
    const args = msg.params?.arguments || {};
    try {
      let out;
      if (name === "sqlite_query") out = handleQuery(args.sql, args.params || []);
      else if (name === "sqlite_execute") out = handleExecute(args.sql, args.params || []);
      else if (name === "sqlite_schema") out = handleSchema(args.table || null);
      else out = `未知工具: ${name}`;
      send({ jsonrpc: "2.0", id: msg.id, result: makeResult(out) });
    } catch (e) {
      send({ jsonrpc: "2.0", id: msg.id, result: makeResult(`工具执行错误: ${e?.message || e}`, true) });
    }
  }
}

process.stdin.on("error", () => {});
