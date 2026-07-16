/**
 * Votek Companion — Method Handlers
 *
 * Each handler maps a JSON-RPC method name to a VS Code Extension API call.
 * All handlers are stateless — they just bridge VS Code API → JSON result.
 */

import * as vscode from "vscode";
import * as path from "path";
import type {
  ActiveEditorResult,
  DiagnosticItem,
  TabItem,
  WorkspaceInfoResult,
  SymbolItem,
  GetDiagnosticsParams,
  OpenFileParams,
  ApplyEditParams,
  ApplyEditResult,
  GetTerminalOutputParams,
  TerminalOutputResult,
  ExecuteCommandParams,
  ExecuteCommandResult,
  GetFileSymbolsParams,
  StatusResult,
  SendToTerminalParams,
  SendToTerminalResult,
  SearchInWorkspaceParams,
  SearchResultItem,
  GetFileDiffParams,
  GetFileDiffResult,
} from "./protocol";

// ── Result wrapper ──

interface HandlerResult {
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

// ── Dispatch ──

export async function handleMethod(method: string, params: Record<string, unknown>): Promise<HandlerResult> {
  try {
    switch (method) {
      case "status":               return { result: handleStatus() };
      case "getActiveEditor":      return { result: await handleGetActiveEditor() };
      case "getDiagnostics":       return { result: await handleGetDiagnostics(params as unknown as GetDiagnosticsParams) };
      case "getOpenTabs":          return { result: await handleGetOpenTabs() };
      case "openFile":             return { result: await handleOpenFile(params as unknown as OpenFileParams) };
      case "applyEdit":            return { result: await handleApplyEdit(params as unknown as ApplyEditParams) };
      case "getTerminalOutput":    return { result: await handleGetTerminalOutput(params as unknown as GetTerminalOutputParams) };
      case "executeCommand":       return { result: await handleExecuteCommand(params as unknown as ExecuteCommandParams) };
      case "getWorkspaceInfo":     return { result: await handleGetWorkspaceInfo() };
      case "getFileSymbols":       return { result: await handleGetFileSymbols(params as unknown as GetFileSymbolsParams) };
      case "sendToTerminal":       return { result: await handleSendToTerminal(params as unknown as SendToTerminalParams) };
      case "searchInWorkspace":    return { result: await handleSearchInWorkspace(params as unknown as SearchInWorkspaceParams) };
      case "getFileDiff":          return { result: await handleGetFileDiff(params as unknown as GetFileDiffParams) };
      default:
        return { error: { code: -32601, message: `Unknown method: ${method}` } };
    }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return { error: { code: -32000, message: `Handler error: ${msg}` } };
  }
}

// ── status ──

function handleStatus(): StatusResult {
  return {
    ok: true,
    version: "0.1.0",
    uptime: 0, // set by caller - not tracked here
  };
}

// ── getActiveEditor ──

async function handleGetActiveEditor(): Promise<ActiveEditorResult | null> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return null;

  const doc = editor.document;
  const sel = editor.selection;

  return {
    filePath: doc.uri.fsPath,
    language: doc.languageId,
    cursorLine: sel.active.line + 1,
    cursorColumn: sel.active.character + 1,
    selectedText: doc.getText(sel),
    totalLines: doc.lineCount,
  };
}

// ── getDiagnostics ──

async function handleGetDiagnostics(params: GetDiagnosticsParams): Promise<DiagnosticItem[]> {
  const diags: DiagnosticItem[] = [];

  // Get diagnostics from all files (or filter by filePath)
  const allDiagnostics = vscode.languages.getDiagnostics();

  for (const [uri, fileDiags] of allDiagnostics) {
    const fp = uri.fsPath;
    if (params.filePath && fp !== params.filePath) continue;

    for (const d of fileDiags) {
      const severity = d.severity === vscode.DiagnosticSeverity.Error ? "error"
        : d.severity === vscode.DiagnosticSeverity.Warning ? "warning"
        : d.severity === vscode.DiagnosticSeverity.Information ? "info"
        : "hint";

      diags.push({
        filePath: fp,
        severity,
        message: d.message,
        line: d.range.start.line + 1,
        column: d.range.start.character + 1,
        source: d.source || "unknown",
        code: d.code ? String(d.code) : undefined,
      });
    }
  }

  // Sort: errors first, then by file
  diags.sort((a, b) => {
    const sev = { error: 0, warning: 1, info: 2, hint: 3 };
    const s = (sev[a.severity] ?? 2) - (sev[b.severity] ?? 2);
    if (s !== 0) return s;
    return a.filePath.localeCompare(b.filePath) || a.line - b.line;
  });

  return diags;
}

// ── getOpenTabs ──

async function handleGetOpenTabs(): Promise<TabItem[]> {
  const tabs: TabItem[] = [];
  const groups = vscode.window.tabGroups;

  for (const group of groups.all) {
    for (const tab of group.tabs) {
      const input = tab.input;
      if (input && typeof input === "object" && "uri" in input) {
        const uri = (input as { uri?: vscode.Uri }).uri;
        if (uri) {
          tabs.push({
            filePath: uri.fsPath,
            language: path.extname(uri.fsPath).replace(".", "") || "plaintext",
            isDirty: tab.isDirty,
            isActive: tab.isActive,
          });
        }
      }
    }
  }

  return tabs;
}

// ── openFile ──

async function handleOpenFile(params: OpenFileParams): Promise<{ success: boolean }> {
  const uri = vscode.Uri.file(params.filePath);

  // Check file exists before trying to open
  try {
    await vscode.workspace.fs.stat(uri);
  } catch {
    return { success: false };
  }

  const doc = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(doc, {
    viewColumn: vscode.ViewColumn.Active,
    preserveFocus: false,
  });

  if (params.line != null) {
    const line = Math.max(0, Math.min(params.line - 1, doc.lineCount - 1));
    const col = params.column != null ? Math.max(0, params.column - 1) : 0;
    const pos = new vscode.Position(line, col);
    editor.selection = new vscode.Selection(pos, pos);
    editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
  }

  return { success: true };
}

// ── applyEdit ──

async function handleApplyEdit(params: ApplyEditParams): Promise<ApplyEditResult> {
  const uri = vscode.Uri.file(params.filePath);

  // Open the file first so the edit is visible
  const doc = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(doc, vscode.ViewColumn.Active);

  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.uri.fsPath !== params.filePath) {
    return { success: false, message: "Could not open target file" };
  }

  const wsEdit = new vscode.WorkspaceEdit();

  for (const edit of params.edits) {
    const range = new vscode.Range(
      new vscode.Position(edit.startLine - 1, edit.startColumn - 1),
      new vscode.Position(edit.endLine - 1, edit.endColumn - 1)
    );
    wsEdit.replace(uri, range, edit.text);
  }

  const applied = await vscode.workspace.applyEdit(wsEdit);
  if (!applied) {
    return { success: false, message: "WorkspaceEdit was not applied" };
  }

  await doc.save();

  return { success: true, message: `Applied ${params.edits.length} edit(s)` };
}

// ── getTerminalOutput ──

async function handleGetTerminalOutput(params: GetTerminalOutputParams): Promise<TerminalOutputResult | null> {
  let term: vscode.Terminal | undefined;

  if (params.name) {
    term = vscode.window.terminals.find((t) => t.name === params.name);
  }

  if (!term) {
    term = vscode.window.activeTerminal;
  }

  if (!term) {
    term = vscode.window.terminals[0];
  }

  if (!term) return null;

  // VS Code doesn't expose terminal content directly via API.
  // We use the clipboard workaround: select-all + copy, then read clipboard.
  // This is a known limitation of the VS Code Extension API.
  const oldClipboard = await vscode.env.clipboard.readText();
  await vscode.commands.executeCommand("workbench.action.terminal.selectAll");
  await vscode.commands.executeCommand("workbench.action.terminal.copySelection");
  await vscode.commands.executeCommand("workbench.action.terminal.clearSelection");

  // Small delay for clipboard to update
  await delay(100);
  const content = await vscode.env.clipboard.readText();

  // Restore clipboard
  await vscode.env.clipboard.writeText(oldClipboard);

  return { name: term.name, content };
}

// ── executeCommand ──

async function handleExecuteCommand(params: ExecuteCommandParams): Promise<ExecuteCommandResult> {
  try {
    const result = await vscode.commands.executeCommand(params.command, ...(params.args || []));
    return {
      success: true,
      result: JSON.stringify(result ?? null),
    };
  } catch (e: unknown) {
    return {
      success: false,
      result: e instanceof Error ? e.message : String(e),
    };
  }
}

// ── getWorkspaceInfo ──

async function handleGetWorkspaceInfo(): Promise<WorkspaceInfoResult | null> {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) return null;

  const root = folders[0];
  const rootPath = root.uri.fsPath;
  const rootName = root.name;

  // Count files (limited to avoid blocking)
  let fileCount = 0;
  try {
    const files = await vscode.workspace.findFiles("**/*", "**/node_modules/**,**/.git/**,**/target/**,**/dist/**,**/.next/**", 5000);
    fileCount = files.length;
  } catch {
    fileCount = -1;
  }

  return {
    name: rootName,
    path: rootPath,
    fileCount,
    folders: folders.map((f) => f.uri.fsPath),
  };
}

// ── getFileSymbols ──

async function handleGetFileSymbols(params: GetFileSymbolsParams): Promise<SymbolItem[]> {
  const uri = vscode.Uri.file(params.filePath);
  const symbols = await vscode.commands.executeCommand<vscode.SymbolInformation[]>(
    "vscode.executeDocumentSymbolProvider",
    uri
  );

  if (!symbols) return [];

  return symbols.map((s) => ({
    name: s.name,
    kind: vscode.SymbolKind[s.kind]?.toLowerCase() || "unknown",
    line: s.location.range.start.line + 1,
    column: s.location.range.start.character + 1,
    containerName: s.containerName || undefined,
  }));
}

// ── sendToTerminal ──

async function handleSendToTerminal(params: SendToTerminalParams): Promise<SendToTerminalResult> {
  let term: vscode.Terminal;

  if (params.newTerminal || !params.terminalName) {
    // 创建新终端
    term = vscode.window.createTerminal(params.terminalName || "Votek Agent");
  } else {
    // 查找现有终端
    term = vscode.window.terminals.find((t) => t.name === params.terminalName)
      || vscode.window.createTerminal(params.terminalName);
  }

  term.show();
  term.sendText(params.text, true); // true = 自动追加换行执行

  return { name: term.name };
}

// ── searchInWorkspace ──

async function handleSearchInWorkspace(params: SearchInWorkspaceParams): Promise<SearchResultItem[]> {
  const results: SearchResultItem[] = [];
  const max = params.maxResults || 50;

  const includePattern = params.include ? `**/${params.include}` : "**/*";

  await vscode.workspace.findTextInFiles(
    { pattern: params.query },
    { include: new vscode.RelativePattern(vscode.workspace.workspaceFolders?.[0] || "", includePattern),
      maxResults: max },
    (item) => {
      for (const match of item.ranges.matches) {
        if (results.length >= max) return;
        const range = match as vscode.Range;
        const line = item.uri.fsPath;
        const lineText = item.textDocument.lineAt(range.start.line).text;
        results.push({
          file: line,
          line: range.start.line + 1,
          column: range.start.character + 1,
          preview: lineText.trim(),
        });
      }
    }
  );

  return results;
}

// ── getFileDiff ──

async function handleGetFileDiff(params: GetFileDiffParams): Promise<GetFileDiffResult | null> {
  try {
    const uri = vscode.Uri.file(params.filePath);

    // 使用 VS Code 的 Git 扩展获取差异
    // 先尝试获取原始文件内容（HEAD 版本）
    const headUri = uri.with({ scheme: "git", path: `HEAD:${params.filePath}` });

    let headContent: string | null = null;
    try {
      const headDoc = await vscode.workspace.openTextDocument(headUri);
      headContent = headDoc.getText();
    } catch {
      // 文件可能不在 git 历史中（新文件）
    }

    if (headContent === null) {
      return null;
    }

    const currentDoc = await vscode.workspace.openTextDocument(uri);
    const currentContent = currentDoc.getText();

    if (headContent === currentContent) {
      return { diff: "" }; // 无差异
    }

    // 生成简易 diff（行级别对比）
    const headLines = headContent.split("\n");
    const currentLines = currentContent.split("\n");
    const diffLines: string[] = [];
    const maxLen = Math.max(headLines.length, currentLines.length);

    for (let i = 0; i < maxLen; i++) {
      const h = headLines[i] ?? "";
      const c = currentLines[i] ?? "";

      if (h === c) {
        diffLines.push(` ${h}`);
      } else if (c === "") {
        // 删除行
        diffLines.push(`-${h}`);
      } else if (h === "" || h === undefined) {
        // 新增行
        diffLines.push(`+${c}`);
      } else {
        diffLines.push(`-${h}`);
        diffLines.push(`+${c}`);
      }
    }

    return { diff: diffLines.join("\n") };
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error(`[votek-companion] getFileDiff error:`, msg);
    return null;
  }
}

// ── Helpers ──

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
