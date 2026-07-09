import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeftIcon, ArrowRightIcon, RefreshIcon,
  HomeIcon, ExternalLinkIcon, GlobeIcon,
} from "../components/Icons";

const DEFAULT_HOME = "https://www.google.com";

/** 安全的 URL 补全：自动补全 https:// */
function normalizeUrl(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) return DEFAULT_HOME;
  // 已经是完整协议
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  // 搜索查询（包含空格或中文字符）→ 跳转 Google 搜索
  if (/\s|[^\x00-\x7F]/.test(trimmed)) {
    return `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`;
  }
  // 域名形式 → 补全 https://
  if (trimmed.includes(".") && !trimmed.includes(" ")) {
    return `https://${trimmed}`;
  }
  // 其他 → Google 搜索
  return `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`;
}

interface NavEntry {
  url: string;
  title: string;
}

export default function BrowserPanel() {
  const { t } = useTranslation();
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const [currentUrl, setCurrentUrl] = useState(DEFAULT_HOME);
  const [inputValue, setInputValue] = useState(DEFAULT_HOME);
  const [history, setHistory] = useState<NavEntry[]>([{ url: DEFAULT_HOME, title: "Home" }]);
  const [historyIndex, setHistoryIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [pageTitle, setPageTitle] = useState("");
  const [iframeBlocked] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const loadTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 清理计时器
  useEffect(() => {
    return () => {
      if (loadTimer.current) clearTimeout(loadTimer.current);
    };
  }, []);

  /** 导航到 URL */
  const navigateTo = useCallback((url: string) => {
    const normalized = normalizeUrl(url);
    setCurrentUrl(normalized);
    setInputValue(normalized);
    setLoading(true);
    setLoadError(false);

    // 追加历史记录
    setHistory((prev) => {
      const newHistory = prev.slice(0, historyIndex + 1);
      // 避免连续重复
      const last = newHistory[newHistory.length - 1];
      if (last && last.url === normalized) return prev;
      newHistory.push({ url: normalized, title: normalized });
      return newHistory;
    });
    setHistoryIndex((prev) => prev + 1);
  }, [historyIndex]);

  /** iframe 加载完成 */
  const handleIframeLoad = useCallback(() => {
    setLoading(false);
    setLoadError(false);
    // 尝试获取 iframe 标题（同源情况下有效）
    try {
      const iframe = iframeRef.current;
      if (iframe?.contentDocument?.title) {
        const title = iframe.contentDocument.title;
        setPageTitle(title);
        // 更新历史中的标题
        setHistory((prev) => {
          const updated = [...prev];
          if (updated[historyIndex]) {
            updated[historyIndex] = { ...updated[historyIndex], title };
          }
          return updated;
        });
      }
    } catch {
      // 跨域无法读取，忽略
    }
  }, [historyIndex]);

  /** iframe 加载错误 */
  const handleIframeError = useCallback(() => {
    setLoading(false);
    setLoadError(true);
  }, []);

  /** 地址栏回车 */
  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      navigateTo(inputValue);
    }
  };

  /** 后退 */
  const goBack = () => {
    if (historyIndex > 0) {
      const newIndex = historyIndex - 1;
      const entry = history[newIndex];
      setHistoryIndex(newIndex);
      setCurrentUrl(entry.url);
      setInputValue(entry.url);
      setLoading(true);
      setLoadError(false);
    }
  };

  /** 前进 */
  const goForward = () => {
    if (historyIndex < history.length - 1) {
      const newIndex = historyIndex + 1;
      const entry = history[newIndex];
      setHistoryIndex(newIndex);
      setCurrentUrl(entry.url);
      setInputValue(entry.url);
      setLoading(true);
      setLoadError(false);
    }
  };

  /** 刷新 */
  const reload = () => {
    setLoading(true);
    setLoadError(false);
    // 强制刷新 iframe
    const iframe = iframeRef.current;
    if (iframe) {
      iframe.src = currentUrl;
    }
  };

  /** 主页 */
  const goHome = () => {
    navigateTo(DEFAULT_HOME);
  };

  /** 外部浏览器打开 */
  const openExternally = () => {
    if (typeof window !== "undefined") {
      window.open(currentUrl, "_blank");
    }
  };

  /** Ctrl+L 聚焦地址栏 */
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "l") {
        e.preventDefault();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const canGoBack = historyIndex > 0;
  const canGoForward = historyIndex < history.length - 1;

  return (
    <div className="browser-panel">
      {/* 工具栏 */}
      <div className="browser-toolbar">
        <div className="browser-nav-buttons">
          <button
            className="browser-nav-btn"
            onClick={goBack}
            disabled={!canGoBack}
            title={t("browser.back")}
          >
            <ArrowLeftIcon size={16} />
          </button>
          <button
            className="browser-nav-btn"
            onClick={goForward}
            disabled={!canGoForward}
            title={t("browser.forward")}
          >
            <ArrowRightIcon size={16} />
          </button>
          <button
            className="browser-nav-btn"
            onClick={reload}
            disabled={loading}
            title={t("browser.reload")}
          >
            <RefreshIcon size={16} className={loading ? "spinning" : ""} />
          </button>
          <button
            className="browser-nav-btn"
            onClick={goHome}
            title={t("browser.home")}
          >
            <HomeIcon size={16} />
          </button>
        </div>

        <div className="browser-address-bar">
          <span className="browser-address-icon">
            {loading ? (
              <span className="browser-spinner" />
            ) : loadError ? (
              <span className="browser-error-dot" />
            ) : (
              <GlobeIcon size={14} />
            )}
          </span>
          <input
            ref={inputRef}
            className="browser-address-input"
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t("browser.addressPlaceholder")}
            spellCheck={false}
            autoCapitalize="off"
          />
          <button
            className="browser-nav-btn browser-external-btn"
            onClick={openExternally}
            title={t("browser.openExternally")}
          >
            <ExternalLinkIcon size={14} />
          </button>
        </div>
      </div>

      {/* 内容区 */}
      <div className="browser-content">
        {iframeBlocked ? (
          <div className="browser-blocked">
            <div className="browser-blocked-icon">
              <GlobeIcon size={48} />
            </div>
            <h3>{t("browser.blockedTitle")}</h3>
            <p>{t("browser.blockedMessage")}</p>
            <div className="browser-blocked-actions">
              <a
                href={currentUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-primary"
              >
                <ExternalLinkIcon size={14} />
                <span>{t("browser.openInBrowser")}</span>
              </a>
            </div>
          </div>
        ) : (
          <>
            {/* 加载指示器 */}
            {loading && (
              <div className="browser-loading-bar">
                <div className="browser-loading-bar-inner" />
              </div>
            )}

            {/* iframe */}
            <iframe
              ref={iframeRef}
              className="browser-iframe"
              src={currentUrl}
              title={pageTitle || currentUrl}
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads"
              onLoad={handleIframeLoad}
              onError={handleIframeError}
            />

            {/* 错误提示 */}
            {loadError && !loading && (
              <div className="browser-error-overlay">
                <div className="browser-error-card">
                  <GlobeIcon size={32} />
                  <p>{t("browser.loadError")}</p>
                  <div className="browser-error-actions">
                    <button className="btn btn-secondary" onClick={reload}>
                      {t("browser.retry")}
                    </button>
                    <button className="btn btn-primary" onClick={openExternally}>
                      <ExternalLinkIcon size={14} />
                      <span>{t("browser.openExternally")}</span>
                    </button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* 底部状态栏 */}
      <div className="browser-statusbar">
        <span className="browser-status-text">
          {pageTitle || currentUrl}
        </span>
        {loading && (
          <span className="browser-status-loading">{t("browser.loading")}</span>
        )}
      </div>
    </div>
  );
}
