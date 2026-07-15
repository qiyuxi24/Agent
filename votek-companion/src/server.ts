/**
 * Votek Companion — WebSocket Server
 *
 * Starts a JSON-RPC 2.0 WebSocket server on 127.0.0.1:{port}.
 * Authenticates clients via shared token from VOTEK_BRIDGE_TOKEN env var.
 * Dispatches method calls to handler modules.
 */

import * as http from "http";
import { WebSocketServer, WebSocket } from "ws";
import type { JsonRpcRequest, JsonRpcResponse } from "./protocol";
import { handleMethod } from "./handlers";

const PORT = parseInt(process.env.VOTEK_BRIDGE_PORT || "19527", 10);
const TOKEN = process.env.VOTEK_BRIDGE_TOKEN || "";

let wss: WebSocketServer | null = null;
let startTime: number = 0;

export function startServer(): Promise<void> {
  return new Promise((resolve, reject) => {
    const server = http.createServer((_req, res) => {
      res.writeHead(200, { "Content-Type": "text/plain" });
      res.end("Votek Companion — OK");
    });

    wss = new WebSocketServer({ server });

    wss.on("connection", (ws: WebSocket, req) => {
      // Authenticate: expect token in query param or header
      const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);
      const clientToken = url.searchParams.get("token") || req.headers["x-bridge-token"] || "";

      if (TOKEN && clientToken !== TOKEN) {
        console.error(`[votek-companion] Rejected: bad auth token`);
        ws.close(4001, "Unauthorized");
        return;
      }

      console.log(`[votek-companion] Client connected`);

      ws.on("message", async (data) => {
        try {
          const raw = typeof data === "string" ? data : data.toString();
          const request: JsonRpcRequest = JSON.parse(raw);

          if (request.jsonrpc !== "2.0") {
            sendError(ws, request.id, -32600, "Invalid Request: jsonrpc must be 2.0");
            return;
          }

          const method = request.method;
          const params = request.params || {};

          const result = await handleMethod(method, params);

          if (result.error) {
            sendError(ws, request.id, result.error.code, result.error.message, result.error.data);
          } else {
            sendResult(ws, request.id, result.result);
          }
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e);
          console.error(`[votek-companion] Message error:`, msg);
          // Can't parse id, close connection
          ws.close(4000, msg);
        }
      });

      ws.on("close", () => {
        console.log(`[votek-companion] Client disconnected`);
      });

      ws.on("error", (err) => {
        console.error(`[votek-companion] WebSocket error:`, err.message);
      });
    });

    server.on("error", (err: NodeJS.ErrnoException) => {
      if (err.code === "EADDRINUSE") {
        reject(new Error(`Port ${PORT} is already in use`));
      } else {
        reject(err);
      }
    });

    server.listen(PORT, "127.0.0.1", () => {
      startTime = Date.now();
      console.log(`[votek-companion] WebSocket server listening on 127.0.0.1:${PORT}`);
      resolve();
    });
  });
}

export function stopServer(): Promise<void> {
  return new Promise((resolve) => {
    if (wss) {
      wss.close(() => {
        console.log(`[votek-companion] Server stopped`);
        resolve();
      });
    } else {
      resolve();
    }
  });
}

function sendResult(ws: WebSocket, id: number, result: unknown): void {
  const response: JsonRpcResponse = { jsonrpc: "2.0", id, result };
  ws.send(JSON.stringify(response));
}

function sendError(ws: WebSocket, id: number, code: number, message: string, data?: unknown): void {
  const response: JsonRpcResponse = {
    jsonrpc: "2.0",
    id,
    error: { code, message, ...(data !== undefined ? { data } : {}) },
  };
  ws.send(JSON.stringify(response));
}
