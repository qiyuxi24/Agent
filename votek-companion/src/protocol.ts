/**
 * Votek Companion — Protocol Types
 *
 * JSON-RPC 2.0 over WebSocket.
 * This is the contract between the VS Code extension and the Votek agent.
 * Both sides must agree on method names and parameter/result shapes.
 */

// ── JSON-RPC 2.0 Envelope ──

export interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number;
  result?: unknown;
  error?: JsonRpcError;
}

export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}

// ── Method: getActiveEditor ──

export interface ActiveEditorResult {
  filePath: string;
  language: string;
  cursorLine: number;    // 1-based
  cursorColumn: number;  // 1-based
  selectedText: string;
  totalLines: number;
}

// ── Method: getDiagnostics ──

export interface GetDiagnosticsParams {
  filePath?: string;  // omit for all files
}

export interface DiagnosticItem {
  filePath: string;
  severity: "error" | "warning" | "info" | "hint";
  message: string;
  line: number;       // 1-based
  column: number;     // 1-based
  source: string;     // e.g. "typescript", "rustc"
  code?: string;      // e.g. "TS2322"
}

// ── Method: getOpenTabs ──

export interface TabItem {
  filePath: string;
  language: string;
  isDirty: boolean;
  isActive: boolean;
}

// ── Method: openFile ──

export interface OpenFileParams {
  filePath: string;
  line?: number;     // 1-based
  column?: number;   // 1-based
}

// ── Method: applyEdit ──

export interface ApplyEditParams {
  filePath: string;
  edits: TextEdit[];
}

export interface TextEdit {
  startLine: number;      // 1-based
  startColumn: number;    // 1-based
  endLine: number;        // 1-based
  endColumn: number;      // 1-based
  text: string;           // replacement text
}

export interface ApplyEditResult {
  success: boolean;
  message: string;
}

// ── Method: getTerminalOutput ──

export interface GetTerminalOutputParams {
  name?: string;  // omit for active terminal
}

export interface TerminalOutputResult {
  name: string;
  content: string;
}

// ── Method: executeCommand ──

export interface ExecuteCommandParams {
  command: string;
  args?: unknown[];
}

export interface ExecuteCommandResult {
  success: boolean;
  result: string;  // JSON-serialized command result
}

// ── Method: getWorkspaceInfo ──

export interface WorkspaceInfoResult {
  name: string;
  path: string;
  fileCount: number;
  folders: string[];
}

// ── Method: getFileSymbols ──

export interface GetFileSymbolsParams {
  filePath: string;
}

export interface SymbolItem {
  name: string;
  kind: string;     // "function", "class", "variable", etc.
  line: number;     // 1-based
  column: number;   // 1-based
  containerName?: string;
}

// ── Status ──

export interface StatusResult {
  ok: boolean;
  version: string;
  uptime: number;  // seconds
}

// ── Method: sendToTerminal ──

export interface SendToTerminalParams {
  text: string;
  terminalName?: string;
  newTerminal?: boolean;
}

export interface SendToTerminalResult {
  name: string;
}

// ── Method: searchInWorkspace ──

export interface SearchInWorkspaceParams {
  query: string;
  include?: string;
  maxResults?: number;
}

export interface SearchResultItem {
  file: string;
  line: number;
  column: number;
  preview: string;
}

// ── Method: getFileDiff ──

export interface GetFileDiffParams {
  filePath: string;
}

export interface GetFileDiffResult {
  diff: string;
}

// ── Method registry ──

export const METHODS = [
  "status",
  "getActiveEditor",
  "getDiagnostics",
  "getOpenTabs",
  "openFile",
  "applyEdit",
  "getTerminalOutput",
  "executeCommand",
  "getWorkspaceInfo",
  "getFileSymbols",
  "sendToTerminal",
  "searchInWorkspace",
  "getFileDiff",
] as const;

export type MethodName = (typeof METHODS)[number];
