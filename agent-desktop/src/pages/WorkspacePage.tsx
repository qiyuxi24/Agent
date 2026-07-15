import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../stores/appStore";
import { FolderIcon, WorkspaceIcon, PlusIcon, DeleteIcon, RefreshIcon } from "../components/Icons";
import "../styles/workspace.css";

// ── 类型 ──

interface SandboxInfo {
  id: string;
  root: string;
  name: string;
}

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
}

// ── 格式化文件大小 ──

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ── 组件 ──

export default function WorkspacePage() {
  const { t } = useTranslation();
  const { workspaceId, workspaceName, workspacePath, setWorkspace, clearWorkspace } = useAppStore();

  // 创建表单
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newPath, setNewPath] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");

  // 已有工作空间列表
  const [workspaces, setWorkspaces] = useState<SandboxInfo[]>([]);
  const [loadingList, setLoadingList] = useState(false);

  // 文件浏览
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [filesLoading, setFilesLoading] = useState(false);
  const [currentDir, setCurrentDir] = useState(".");

  // 新建文件/目录
  const [showNewFile, setShowNewFile] = useState(false);
  const [showNewDir, setShowNewDir] = useState(false);
  const [newItemName, setNewItemName] = useState("");

  // 确认删除
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  // ── 加载工作空间列表 ──
  const loadWorkspaces = useCallback(async () => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    setLoadingList(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const list = await invoke<SandboxInfo[]>("workspace_list");
      setWorkspaces(list);
      // 同步当前空间状态
      const cur = await invoke<SandboxInfo | null>("workspace_get_current");
      if (cur) {
        setWorkspace(cur.id, cur.name, cur.root);
      }
    } catch (e) {
      console.error("加载工作空间列表失败:", e);
    } finally {
      setLoadingList(false);
    }
  }, [setWorkspace]);

  // ── 加载文件列表 ──
  const loadFiles = useCallback(async (dir = ".") => {
    if (!workspaceId) return;
    setFilesLoading(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const entries = await invoke<FileEntry[]>("sandbox_list_dir", { path: dir });
      setFiles(entries);
      setCurrentDir(dir);
    } catch (e) {
      console.error("加载文件列表失败:", e);
    } finally {
      setFilesLoading(false);
    }
  }, [workspaceId]);

  // 初始化
  useEffect(() => {
    loadWorkspaces();
  }, [loadWorkspaces]);

  useEffect(() => {
    if (workspaceId) {
      loadFiles(".");
    }
  }, [workspaceId, loadFiles]);

  // ── 创建空间 ──
  const handleCreate = async () => {
    if (!newPath.trim()) { setError(t("workspace.folderPlaceholder")); return; }
    setCreating(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const name = newName.trim() || newPath.split(/[/\\]/).pop() || "未命名";
      const info = await invoke<SandboxInfo>("workspace_create", { name, path: newPath });
      setWorkspace(info.id, info.name, info.root);
      setShowCreate(false);
      setNewName("");
      setNewPath("");
      loadWorkspaces();
    } catch (e: any) {
      setError(typeof e === "string" ? e : e?.message || "创建失败");
    } finally {
      setCreating(false);
    }
  };

  // ── 切换空间 ──
  const handleSwitch = async (id: string) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("workspace_set_current", { id });
      const ws = workspaces.find((w) => w.id === id);
      if (ws) setWorkspace(ws.id, ws.name, ws.root);
      setCurrentDir(".");
    } catch (e) {
      console.error("切换工作空间失败:", e);
    }
  };

  // ── 删除空间 ──
  const handleRemove = async (id: string) => {
    if (!confirm(t("workspace.removeConfirm"))) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("workspace_remove", { id });
      if (workspaceId === id) clearWorkspace();
      loadWorkspaces();
    } catch (e) {
      console.error("删除工作空间失败:", e);
    }
  };

  // ── 选择文件夹 ──
  const handlePickFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false, title: t("workspace.selectFolder") });
      if (selected && typeof selected === "string") {
        setNewPath(selected);
        if (!newName.trim()) {
          setNewName(selected.split(/[/\\]/).pop() || "");
        }
      }
    } catch (e) {
      // 降级：在非 Tauri 环境不弹窗
      console.warn("文件选择器不可用:", e);
    }
  };

  // ── 文件操作 ──
  const handleCreateFile = async () => {
    if (!newItemName.trim()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const filePath = currentDir === "." ? newItemName : `${currentDir}/${newItemName}`;
      await invoke("sandbox_write_file", { path: filePath, content: "" });
      setShowNewFile(false);
      setNewItemName("");
      loadFiles(currentDir);
    } catch (e) {
      console.error("创建文件失败:", e);
    }
  };

  const handleCreateDir = async () => {
    if (!newItemName.trim()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const dirPath = currentDir === "." ? newItemName : `${currentDir}/${newItemName}`;
      await invoke("sandbox_create_dir", { path: dirPath });
      setShowNewDir(false);
      setNewItemName("");
      loadFiles(currentDir);
    } catch (e) {
      console.error("创建目录失败:", e);
    }
  };

  const handleDelete = async (entry: FileEntry) => {
    if (!confirm(t("workspace.deleteConfirm", { name: entry.name }))) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("sandbox_delete_file", { path: entry.path });
      setDeleteTarget(null);
      loadFiles(currentDir);
    } catch (e) {
      console.error("删除失败:", e);
    }
  };

  const handleNavigate = (entry: FileEntry) => {
    if (entry.is_dir) {
      loadFiles(entry.path);
    }
  };

  const handleGoUp = () => {
    if (currentDir === ".") return;
    const parent = currentDir.split("/").slice(0, -1).join("/") || ".";
    loadFiles(parent);
  };

  const handleOpenInExplorer = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("ide_set_workspace", { path: workspacePath });
    } catch {
      // fallback
    }
  };

  // ── 无工作空间时显示创建界面 ──
  if (!workspaceId) {
    return (
      <div className="workspace-page">
        <div className="workspace-empty">
          <WorkspaceIcon size={48} />
          <h2>{t("workspace.title")}</h2>
          <p className="workspace-desc">{t("workspace.desc")}</p>

          {workspaces.length > 0 && (
            <div className="workspace-existing">
              <h3>{t("workspace.switchWorkspace")}</h3>
              <div className="workspace-list">
                {workspaces.map((ws) => (
                  <div key={ws.id} className="workspace-card">
                    <div className="workspace-card-info" onClick={() => handleSwitch(ws.id)}>
                      <FolderIcon size={20} />
                      <div>
                        <div className="workspace-card-name">{ws.name}</div>
                        <div className="workspace-card-path">{ws.root}</div>
                      </div>
                    </div>
                    <button className="btn btn-icon btn-danger" onClick={() => handleRemove(ws.id)} title={t("workspace.remove")}>
                      <DeleteIcon size={14} />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {!showCreate ? (
            <button className="btn btn-primary workspace-create-btn" onClick={() => setShowCreate(true)}>
              <PlusIcon size={18} />
              {t("workspace.create")}
            </button>
          ) : (
            <div className="workspace-create-form">
              <label>{t("workspace.nameLabel")}</label>
              <input
                className="input"
                placeholder={t("workspace.namePlaceholder")}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
              />
              <label>{t("workspace.selectFolder")}</label>
              <div className="workspace-path-row">
                <input
                  className="input"
                  placeholder={t("workspace.folderPlaceholder")}
                  value={newPath}
                  onChange={(e) => setNewPath(e.target.value)}
                />
                <button className="btn btn-secondary" onClick={handlePickFolder}>
                  <FolderIcon size={16} />
                </button>
              </div>
              {error && <p className="workspace-error">{error}</p>}
              <div className="workspace-create-actions">
                <button className="btn btn-secondary" onClick={() => { setShowCreate(false); setError(""); }}>
                  {t("settings.models.cancel")}
                </button>
                <button className="btn btn-primary" onClick={handleCreate} disabled={creating}>
                  {creating ? t("workspace.creating") : t("workspace.create")}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  // ── 有工作空间时显示文件管理器 ──
  return (
    <div className="workspace-page">
      {/* 顶部信息栏 */}
      <div className="workspace-header">
        <div className="workspace-header-info">
          <FolderIcon size={20} />
          <div>
            <h2>{workspaceName}</h2>
            <span className="workspace-header-path">{workspacePath}</span>
          </div>
        </div>
        <div className="workspace-header-actions">
          <button className="btn btn-secondary btn-sm" onClick={handleOpenInExplorer} title={t("workspace.openFolder")}>
            {t("workspace.openFolder")}
          </button>
          <button className="btn btn-secondary btn-sm" onClick={() => clearWorkspace()} title={t("workspace.switchWorkspace")}>
            {t("workspace.switchWorkspace")}
          </button>
        </div>
      </div>

      {/* 工具栏 */}
      <div className="workspace-toolbar">
        <div className="workspace-breadcrumb">
          <button className="breadcrumb-btn" onClick={handleGoUp} disabled={currentDir === "."}>
            ↑
          </button>
          <span className="breadcrumb-path">{currentDir === "." ? workspaceName : currentDir}</span>
        </div>
        <div className="workspace-toolbar-actions">
          <button className="btn btn-secondary btn-sm" onClick={() => loadFiles(currentDir)} title={t("workspace.refresh")}>
            <RefreshIcon size={14} />
          </button>
          <button className="btn btn-secondary btn-sm" onClick={() => { setShowNewFile(true); setShowNewDir(false); setNewItemName(""); }}>
            {t("workspace.createFile")}
          </button>
          <button className="btn btn-secondary btn-sm" onClick={() => { setShowNewDir(true); setShowNewFile(false); setNewItemName(""); }}>
            {t("workspace.createDir")}
          </button>
        </div>
      </div>

      {/* 新建输入框 */}
      {(showNewFile || showNewDir) && (
        <div className="workspace-new-item">
          <input
            className="input"
            placeholder={showNewFile ? t("workspace.fileNamePlaceholder") : t("workspace.dirNamePlaceholder")}
            value={newItemName}
            onChange={(e) => setNewItemName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") showNewFile ? handleCreateFile() : handleCreateDir();
              if (e.key === "Escape") { setShowNewFile(false); setShowNewDir(false); setNewItemName(""); }
            }}
            autoFocus
          />
          <button className="btn btn-primary btn-sm" onClick={showNewFile ? handleCreateFile : handleCreateDir}>
            {t("settings.models.confirm")}
          </button>
        </div>
      )}

      {/* 文件列表 */}
      <div className="workspace-files">
        {filesLoading ? (
          <p className="workspace-loading">{t("browser.loading")}</p>
        ) : files.length === 0 ? (
          <p className="workspace-empty-dir">{t("workspace.emptyDir")}</p>
        ) : (
          files.map((entry) => (
            <div
              key={entry.path}
              className={`workspace-file-item ${entry.is_dir ? "is-dir" : ""}`}
              onClick={() => handleNavigate(entry)}
            >
              <FolderIcon size={16} />
              <span className="workspace-file-name">{entry.name}</span>
              <span className="workspace-file-size">{entry.is_dir ? "" : formatSize(entry.size)}</span>
              <button
                className="btn btn-icon workspace-file-delete"
                onClick={(e) => { e.stopPropagation(); handleDelete(entry); }}
                title={t("workspace.deleteFile")}
              >
                <DeleteIcon size={12} />
              </button>
            </div>
          ))
        )}
      </div>

      {/* 底部信息 */}
      <div className="workspace-footer">
        {t("workspace.files")}: {files.length}
      </div>
    </div>
  );
}
