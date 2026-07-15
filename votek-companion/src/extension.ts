/**
 * Votek Companion — Extension Entry Point
 *
 * Activates on "*" (every VS Code window) and starts the WebSocket bridge server.
 * Reads VOTEK_BRIDGE_PORT and VOTEK_BRIDGE_TOKEN from environment variables
 * (set by Votek's code_server.rs before spawning code-server).
 */

import * as vscode from "vscode";
import { startServer, stopServer } from "./server";

export function activate(context: vscode.ExtensionContext): void {
  const port = process.env.VOTEK_BRIDGE_PORT || "19527";
  const token = process.env.VOTEK_BRIDGE_TOKEN || "";

  console.log(`[votek-companion] Activating on port ${port}` + (token ? " (auth enabled)" : ""));

  // Start the WebSocket server
  startServer().then(() => {
    console.log(`[votek-companion] Ready — Votek agent can now connect`);
  }).catch((err: Error) => {
    console.error(`[votek-companion] Failed to start server: ${err.message}`);
    vscode.window.showErrorMessage(`Votek Companion: ${err.message}`);
  });

  // Register deactivation
  context.subscriptions.push({
    dispose: () => {
      stopServer().catch(console.error);
    },
  });

  // Register a status bar item so users know it's running
  const statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100
  );
  statusBar.text = "$(hubot) Votek Bridge";
  statusBar.tooltip = `Votek AI agent bridge active on port ${port}`;
  statusBar.show();
  context.subscriptions.push(statusBar);
}

export function deactivate(): void {
  stopServer().catch(console.error);
}
