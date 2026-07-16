import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  DatabaseIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  UploadIcon,
  FileTextIcon,
  PlayIcon,
  SparklesIcon,
  FolderIcon,
  ModelIcon,
  TrashIcon,
} from "../../components/Icons";

// ========== 类型定义（与 Rust 端对齐） ==========

interface RagConfig {
  enabled: boolean;
  db_path: string;
  embedding_model: string;
  embedding_dimension: number;
  chunk_size: number;
  chunk_overlap: number;
  top_k: number;
}

interface RagStats {
  document_count: number;
  chunk_count: number;
}

interface SearchResultItem {
  document_id: string;
  content: string;
  score: number;
  source_type: string;
  source_id: string;
  chunk_index: number;
}

interface RagDocument {
  id: string;
  content: string;
  source_type: string;
  source_id: string;
}

interface UploadFileResult {
  document_id: string;
  source_name: string;
  file_type: string;
  text_length: number;
  chunk_count: number;
}

// ========== 文件上传拖拽状态 ==========
const DRAG_ACTIVE_CLASS = "rag-dropzone-active";

// ========== 嵌入模型选项（静态，不翻译）==========
const MODEL_OPTIONS = [
  { value: "BAAI/bge-small-zh-v1.5",              label: "BGE Small 中文 (512维, 47MB, 推荐)" },
  { value: "BAAI/bge-large-zh-v1.5",              label: "BGE Large 中文 (1024维, 高精度)" },
  { value: "BAAI/bge-m3",                         label: "BGE-M3 多语言 (1024维)" },
  { value: "sentence-transformers/all-MiniLM-L6-v2", label: "MiniLM (英文, 384维, 极速)" },
];

// ========== 组件 ==========

export default function RagPanel() {
  const { t } = useTranslation();

  // 状态
  const [initialized, setInitialized] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [stats, setStats] = useState<RagStats>({ document_count: 0, chunk_count: 0 });

  // 配置
  const [dbPath, setDbPath] = useState("");
  const [embeddingModel, setEmbeddingModel] = useState("BAAI/bge-small-zh-v1.5");
  const [chunkSize, setChunkSize] = useState(512);
  const [topK, setTopK] = useState(5);

  // 文档上传
  const [docContent, setDocContent] = useState("");
  const [docSourceName, setDocSourceName] = useState("");
  const [documents, setDocuments] = useState<{ id: string; source_id: string; source_type: string; preview: string }[]>([]);
  const [showDocList, setShowDocList] = useState(false);

  // 文件上传
  const [uploading, setUploading] = useState(false);
  const [dragOver, setDragOver] = useState(false);

  // 检索测试
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResultItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [showSearch, setShowSearch] = useState(false);

  // 高级设置
  const [showAdvanced, setShowAdvanced] = useState(false);

  // ===== Init / Refresh =====
  const initRag = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("rag_init", {
        config: {
          enabled: true,
          db_path: dbPath || "./data/rag_db",
          embedding_model: embeddingModel,
          embedding_dimension: 512,
          chunk_size: chunkSize,
          chunk_overlap: 0,
          top_k: topK,
        },
      });
      setInitialized(true);
      await refreshStats();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  }, [dbPath, embeddingModel, chunkSize, topK]);

  const refreshStats = useCallback(async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const s = await invoke<RagStats>("rag_get_stats");
      setStats(s);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    // 自动检测是否已初始化
    (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const config = await invoke<RagConfig>("rag_get_config");
        if (config.enabled) {
          setInitialized(true);
          setDbPath(config.db_path);
          setEmbeddingModel(config.embedding_model);
          setChunkSize(config.chunk_size);
          setTopK(config.top_k);
          await refreshStats();
        }
      } catch {
        // 未初始化，保持默认
      }
    })();
  }, [refreshStats]);

  // ===== 上传文档 =====
  const uploadDocument = async () => {
    if (!docContent.trim() || !docSourceName.trim()) {
      setError(t("settings.rag.needFields"));
      return;
    }
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const doc: RagDocument = {
        id: `${Date.now()}_${docSourceName}`,
        content: docContent,
        source_type: "user_upload",
        source_id: docSourceName,
      };
      const count = await invoke<number>("rag_index_documents", {
        request: { documents: [doc] },
      });
      setDocContent("");
      setDocSourceName("");
      setDocuments((prev) => [
        {
          id: doc.id,
          source_id: docSourceName,
          source_type: "user_upload",
          preview: docContent.slice(0, 100) + (docContent.length > 100 ? "..." : ""),
        },
        ...prev,
      ]);
      setShowDocList(true);
      setError(t("settings.rag.indexSuccess", { count }));
      await refreshStats();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  // ===== 上传文件 =====
  const uploadFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "Documents",
            extensions: ["txt", "md", "pdf", "docx"],
          },
        ],
      });
      if (!selected) return; // 用户取消

      const files = Array.isArray(selected) ? selected : [selected];
      setUploading(true);
      setError("");
      const { invoke } = await import("@tauri-apps/api/core");

      for (const filePath of files) {
        try {
          const result = await invoke<UploadFileResult>("rag_upload_file", {
            filePath,
          });
          setDocuments((prev) => [
            {
              id: result.document_id,
              source_id: result.source_name,
              source_type: `file_${result.file_type}`,
              preview: `[${result.file_type.toUpperCase()}] ${result.source_name} (${result.chunk_count} ${t("settings.rag.chunks")})`,
            },
            ...prev,
          ]);
          setShowDocList(true);
          setError(t("settings.rag.uploadSuccess", {
            name: result.source_name,
            chunks: result.chunk_count,
          }));
        } catch (e: unknown) {
          const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
          setError(t("settings.rag.uploadError", { error: msg }));
        }
      }
      await refreshStats();
    } catch (e: unknown) {
      setError(t("settings.rag.uploadError", {
        error: typeof e === "string" ? e : (e as Error)?.message || String(e),
      }));
    } finally {
      setUploading(false);
    }
  };

  // ===== 拖拽上传 =====
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);

    const droppedFiles = Array.from(e.dataTransfer.files);
    if (droppedFiles.length === 0) return;

    // 过滤支持的文件类型
    const supported = droppedFiles.filter((f) => {
      const name = f.name.toLowerCase();
      return name.endsWith(".txt") || name.endsWith(".md") || name.endsWith(".pdf") || name.endsWith(".docx");
    });

    if (supported.length === 0) {
      setError(t("settings.rag.uploadError", {
        error: t("settings.rag.supportedFormats"),
      }));
      return;
    }

    setUploading(false);

    // 浏览器 File 对象无法直接传给 Rust（没有文件路径），
    // 调用 uploadFile 打开原生对话框作为替代
    uploadFile();
  };

  // ===== 删除文档 =====
  const deleteDocument = async (docId: string, sourceName: string) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("rag_delete_document", { docId });
      setDocuments((prev) => prev.filter((d) => d.id !== docId));
      setError(t("settings.rag.deleted", { name: sourceName }));
      await refreshStats();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    }
  };

  // ===== 检索测试 =====
  const doSearch = async () => {
    if (!searchQuery.trim()) return;
    setSearching(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const results = await invoke<SearchResultItem[]>("rag_search", {
        query: { query: searchQuery, top_k: topK, source_type_filter: null },
      });
      setSearchResults(results);
      setShowSearch(true);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setSearching(false);
    }
  };

  // ===== 清空索引 =====
  const clearAll = async () => {
    if (!window.confirm(t("settings.rag.clearConfirm"))) return;
    setBusy(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("rag_clear_all");
      setDocuments([]);
      setSearchResults([]);
      setError(t("settings.rag.cleared"));
      await refreshStats();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  // ===== 快速索引示例文本 =====
  const indexDemoText = async () => {
    const demos = [
      {
        id: `demo_${Date.now()}_0`,
        content:
          "Tauri 是一个用于构建跨平台桌面应用的框架，前端使用 Web 技术（HTML/CSS/JS），后端使用 Rust。Tauri 的核心优势在于极小的包体积和极低的内存占用，相比 Electron 可以节省 90% 以上的资源。",
        source_type: "demo",
        source_id: "Tauri 介绍",
      },
      {
        id: `demo_${Date.now()}_1`,
        content:
          "RAG（检索增强生成）是一种结合信息检索和文本生成的技术。它将用户查询与知识库中的相关文档进行语义匹配，然后将检索到的上下文注入 LLM 的提示词中，从而大幅提高回答的准确性和时效性。",
        source_type: "demo",
        source_id: "RAG 概念",
      },
      {
        id: `demo_${Date.now()}_2`,
        content:
          "BGE (BAAI General Embedding) 是智源研究院开源的中文嵌入模型系列，包括 bge-small-zh-v1.5（512维、轻量快速）和 bge-large-zh-v1.5（1024维、高精度），在中文语义检索任务上表现优异。",
        source_type: "demo",
        source_id: "BGE 嵌入模型",
      },
    ];
    setBusy(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const count = await invoke<number>("rag_index_documents", {
        request: { documents: demos },
      });
      setError(t("settings.rag.demoSuccess", { count }));
      setShowDocList(true);
      setDocuments((prev) => [
        ...demos.map((d) => ({
          id: d.id,
          source_id: d.source_id,
          source_type: d.source_type,
          preview: d.content.slice(0, 80) + "...",
        })),
        ...prev,
      ]);
      await refreshStats();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  // 判断错误是否为成功消息（用于样式区分）
  const isSuccessMsg = error.includes(t("settings.rag.indexSuccess", { count: "" }).split(":")[0])
    || error.includes(t("settings.rag.deleted", { name: "" }).split(":")[0])
    || error.includes(t("settings.rag.demoSuccess", { count: "" }).split(":")[0])
    || error.includes(t("settings.rag.cleared"))
    || (error.startsWith(t("settings.rag.uploadSuccess", { name: "", chunks: "" }).split("'")[0]) && !error.includes(t("settings.rag.uploadError", { error: "" }).split(":")[0]));

  return (
    <section className="settings-panel rag-panel">
      {/* ===== 初始化区域 ===== */}
      <div className="rag-section">
        <div className="section-header">
          <h3>
            <DatabaseIcon size={18} />
            {t("settings.rag.status")}
          </h3>
          {initialized && (
            <span className="rag-status on">
              <span className="mcp-status-dot" />
              {t("settings.rag.active")}
            </span>
          )}
        </div>

        {!initialized ? (
          <div className="rag-init-card">
            <div className="rag-init-icon">
              <DatabaseIcon size={48} />
            </div>
            <h4>{t("settings.rag.enableTitle")}</h4>
            <p className="panel-desc">{t("settings.rag.desc")}</p>
            <div className="rag-init-form">
              <div className="form-group">
                <label>{t("settings.rag.dbPath")}</label>
                <input
                  value={dbPath}
                  onChange={(e) => setDbPath(e.target.value)}
                  placeholder={t("settings.rag.dbPathPlaceholder")}
                />
              </div>
              <div className="form-group">
                <label>{t("settings.rag.model")}</label>
                <select value={embeddingModel} onChange={(e) => setEmbeddingModel(e.target.value)}>
                  {MODEL_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>{opt.label}</option>
                  ))}
                </select>
              </div>
              <button className="btn btn-primary" onClick={initRag} disabled={busy}>
                <SparklesIcon size={14} />
                {busy ? t("settings.rag.initializing") : t("settings.rag.enableBtn")}
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* 统计栏 */}
            <div className="rag-stats-bar">
              <div className="rag-stat-item">
                <FileTextIcon size={16} />
                <div>
                  <span className="rag-stat-value">{stats.chunk_count}</span>
                  <span className="rag-stat-label">{t("settings.rag.chunks")}</span>
                </div>
              </div>
              <div className="rag-stat-item">
                <FolderIcon size={16} />
                <div>
                  <span className="rag-stat-value">{documents.length}</span>
                  <span className="rag-stat-label">{t("settings.rag.documents")}</span>
                </div>
              </div>
              <div className="rag-stat-item">
                <ModelIcon size={16} />
                <div>
                  <span className="rag-stat-label">{t("settings.rag.modelLabel")}</span>
                  <span className="rag-stat-sub">
                    {embeddingModel.split("/").pop()}
                  </span>
                </div>
              </div>
              <button className="btn btn-icon-sm" onClick={refreshStats} title={t("settings.rag.refresh")}>
                <RefreshIcon size={14} />
              </button>
            </div>
          </>
        )}
      </div>

      {/* ===== 已初始化后的功能区 ===== */}
      {initialized && (
        <>
          {/* 上传文档 */}
          <div className="rag-section">
            <div className="section-header">
              <h3>
                <UploadIcon size={16} />
                {t("settings.rag.addKnowledge")}
              </h3>
              <div className="rag-header-actions">
                <button
                  className="btn btn-sm btn-outline"
                  onClick={indexDemoText}
                  disabled={busy}
                  title={t("settings.rag.demoData")}
                >
                  <SparklesIcon size={12} />
                  {t("settings.rag.demoData")}
                </button>
              </div>
            </div>
            <div className="rag-upload-form">
              <div className="form-group">
                <label>{t("settings.rag.sourceName")}</label>
                <input
                  value={docSourceName}
                  onChange={(e) => setDocSourceName(e.target.value)}
                  placeholder={t("settings.rag.sourceNamePlaceholder")}
                />
              </div>
              <div className="form-group">
                <label>{t("settings.rag.content")}</label>
                <textarea
                  value={docContent}
                  onChange={(e) => setDocContent(e.target.value)}
                  placeholder={t("settings.rag.contentPlaceholder")}
                  rows={5}
                  className="rag-textarea"
                />
              </div>
              <button
                className="btn btn-primary"
                onClick={uploadDocument}
                disabled={busy || !docContent.trim() || !docSourceName.trim()}
              >
                <PlusIcon size={14} />
                {t("settings.rag.indexDoc")}
              </button>
            </div>
          </div>

          {/* 上传文件 */}
          <div className="rag-section">
            <div className="section-header">
              <h3>
                <FileTextIcon size={16} />
                {t("settings.rag.uploadFile")}
              </h3>
              <button
                className="btn btn-sm btn-outline"
                onClick={uploadFile}
                disabled={uploading}
              >
                <UploadIcon size={12} />
                {uploading ? t("settings.rag.uploading") : t("settings.rag.uploadFile")}
              </button>
            </div>
            <p className="panel-desc">{t("settings.rag.uploadFileHint")}</p>
            <div
              className={`rag-dropzone ${dragOver ? DRAG_ACTIVE_CLASS : ""}`}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
            >
              <FileTextIcon size={24} />
              <p>{t("settings.rag.dragDrop")}</p>
              <span className="rag-dropzone-hint">{t("settings.rag.supportedFormats")}</span>
            </div>
          </div>

          {/* 已索引文档列表 */}
          {documents.length > 0 && (
            <div className="rag-section">
              <div
                className="section-header rag-collapsible"
                onClick={() => setShowDocList(!showDocList)}
              >
                <h3>
                  <FileTextIcon size={16} />
                  {t("settings.rag.indexedDocs")} ({documents.length})
                </h3>
                <span className="rag-toggle">{showDocList ? "▲" : "▼"}</span>
              </div>
              {showDocList && (
                <div className="rag-doc-list">
                  {documents.map((doc) => (
                    <div key={doc.id} className="rag-doc-card">
                      <div className="rag-doc-info">
                        <span className="rag-doc-name">{doc.source_id}</span>
                        <span className="rag-doc-type">{doc.source_type}</span>
                      </div>
                      <div className="rag-doc-preview">{doc.preview}</div>
                      <button
                        className="btn btn-icon-sm btn-danger"
                        onClick={() => deleteDocument(doc.id, doc.source_id)}
                        title={t("settings.rag.indexDoc")}
                      >
                        <TrashIcon size={12} />
                      </button>
                    </div>
                  ))}
                  <button className="btn btn-sm btn-danger-outline" onClick={clearAll}>
                    <TrashIcon size={12} />
                    {t("settings.rag.clearAll")}
                  </button>
                </div>
              )}
            </div>
          )}

          {/* 检索测试 */}
          <div className="rag-section">
            <div
              className="section-header rag-collapsible"
              onClick={() => setShowSearch(!showSearch)}
            >
              <h3>
                <SearchIcon size={16} />
                {t("settings.rag.searchTest")}
              </h3>
              <span className="rag-toggle">{showSearch ? "▲" : "▼"}</span>
            </div>
            <div className="rag-search-bar">
              <SearchIcon size={14} className="rag-search-icon" />
              <input
                className="rag-search-input"
                placeholder={t("settings.rag.searchPlaceholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && doSearch()}
              />
              <button
                className="btn btn-sm btn-primary"
                onClick={doSearch}
                disabled={searching || !searchQuery.trim()}
              >
                <PlayIcon size={12} />
                {searching ? t("settings.rag.searching") : t("settings.rag.search")}
              </button>
            </div>
            {showSearch && searchResults.length > 0 && (
              <div className="rag-results">
                {searchResults.map((r, i) => (
                  <div key={`${r.document_id}_${r.chunk_index}`} className="rag-result-card">
                    <div className="rag-result-header">
                      <span className="rag-result-rank">#{i + 1}</span>
                      <span className="rag-result-score">
                        {t("settings.rag.relevance")}: {(r.score * 100).toFixed(0)}%
                      </span>
                      <span className="rag-result-source">
                        {r.source_type}/{r.source_id}
                      </span>
                    </div>
                    <div className="rag-result-content">{r.content}</div>
                  </div>
                ))}
              </div>
            )}
            {showSearch && searchResults.length === 0 && searchQuery && (
              <div className="placeholder-section">
                <p>{t("settings.rag.noResults")}</p>
              </div>
            )}
          </div>

          {/* 高级设置 */}
          <div className="rag-section">
            <div
              className="section-header rag-collapsible"
              onClick={() => setShowAdvanced(!showAdvanced)}
            >
              <h3>{t("settings.rag.advanced")}</h3>
              <span className="rag-toggle">{showAdvanced ? "▲" : "▼"}</span>
            </div>
            {showAdvanced && (
              <div className="rag-advanced-form">
                <div className="form-group">
                  <label>{t("settings.rag.chunkSize")}: {chunkSize}</label>
                  <input
                    type="range"
                    min="128"
                    max="2048"
                    step="64"
                    value={chunkSize}
                    onChange={(e) => setChunkSize(Number(e.target.value))}
                  />
                  <span className="form-hint">{t("settings.rag.chunkSizeHint")}</span>
                </div>
                <div className="form-group">
                  <label>{t("settings.rag.topK")}: {topK}</label>
                  <input
                    type="range"
                    min="1"
                    max="20"
                    value={topK}
                    onChange={(e) => setTopK(Number(e.target.value))}
                  />
                  <span className="form-hint">{t("settings.rag.topKHint")}</span>
                </div>
                <button className="btn btn-secondary" onClick={initRag} disabled={busy}>
                  <RefreshIcon size={12} />
                  {t("settings.rag.applySettings")}
                </button>
              </div>
            )}
          </div>
        </>
      )}

      {/* 错误/成功提示 */}
      {error && (
        <div className={`rag-error ${isSuccessMsg ? "rag-success" : ""}`}>
          {error}
        </div>
      )}
    </section>
  );
}
