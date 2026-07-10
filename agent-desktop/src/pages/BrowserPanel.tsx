import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  ArrowLeftIcon, ArrowRightIcon, RefreshIcon,
  HomeIcon, ExternalLinkIcon, GlobeIcon,
} from "../components/Icons";

const DEFAULT_HOME = "https://www.bing.com";

/** URL 标准化：补全 https://，中文/空格 → Bing 搜索 */
function normalizeUrl(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) return DEFAULT_HOME;
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  if (/\s|[^\x00-\x7F]/.test(trimmed)) {
    return `https://www.bing.com/search?q=${encodeURIComponent(trimmed)}`;
  }
  if (trimmed.includes(".") && !trimmed.includes(" ")) {
    return `https://${trimmed}`;
  }
  return `https://www.bing.com/search?q=${encodeURIComponent(trimmed)}`;
}

/** 从 URL 提取标题（域名 + 首段路径） */
function urlToTitle(url: string): string {
  try {
    const u = new URL(url);
    const host = u.hostname.replace(/^www\./, "");
    const path = u.pathname.replace(/\/$/, "").split("/").pop() || "";
    return path ? `${host} › ${decodeURIComponent(path)}` : host;
  } catch {
    return url;
  }
}

export default function BrowserPanel() {
  const { t } = useTranslation();
  const contentRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const unlistenRef = useRef<UnlistenFn[]>([]);
  const createdRef = useRef(false);

  const [currentUrl, setCurrentUrl] = useState(DEFAULT_HOME);
  const [inputValue, setInputValue] = useState("");
  const [pageTitle, setPageTitle] = useState("");
  const [loading, setLoading] = useState(false);
  const [canGoBack, setCanGoBack] = useState(false);
  const [canGoForward, setCanGoForward] = useState(false);
  // WebView2 历史栈通过 URL 变化次数推断
  const historyStack = useRef<string[]>([]);
  const historyPos = useRef(-1);

  /** 计算内容区窗口坐标并创建/调整 webview */
  const syncBounds = useCallback(() => {
    if (!contentRef.current) return;
    const rect = contentRef.current.getBoundingClientRect();
    const x = rect.left;
    const y = rect.top;
    const w = rect.width;
    const h = rect.height;

    if (w <= 0 || h <= 0) return;

    if (!createdRef.current) {
      // 首次创建
      createdRef.current = true;
      invoke("browser_create", { url: currentUrl, x, y, w: Math.floor(w), h: Math.floor(h) })
        .catch((e) => console.error("[Browser] 创建失败:", e));
    } else {
      invoke("browser_resize", { x, y, w: Math.floor(w), h: Math.floor(h) })
        .catch(() => {}); // 静默失败
    }
  }, [currentUrl]);

  /** 创建 + 监听事件 + ResizeObserver */
  useEffect(() => {
    let obs: ResizeObserver | null = null;

    const setup = async () => {
      // 监听 Tauri 事件
      try {
        const u1 = await listen<{ url: string }>("browser-url-changed", (event) => {
          const url = event.payload.url;
          setCurrentUrl(url);
          setInputValue(url);
          setPageTitle(urlToTitle(url));
          setLoading(false);

          // 更新历史栈
          const stack = historyStack.current;
          const pos = historyPos.current;
          // 如果当前在历史中间位置，截断后续
          if (pos < stack.length - 1) {
            stack.splice(pos + 1);
          }
          // 避免连续重复
          if (stack[stack.length - 1] !== url) {
            stack.push(url);
            historyPos.current = stack.length - 1;
          }
          setCanGoBack(historyPos.current > 0);
          setCanGoForward(false);
        });
        const u2 = await listen<{ url: string }>("browser-page-loaded", (_event) => {
          setLoading(false);
        });
        unlistenRef.current = [u1, u2];
      } catch (e) {
        console.error("[Browser] 事件监听失败:", e);
      }

      // ResizeObserver 跟踪内容区大小变化
      if (contentRef.current) {
        obs = new ResizeObserver(() => {
          syncBounds();
        });
        obs.observe(contentRef.current);
      }

      // 首次创建
      syncBounds();
    };

    // 延迟执行，确保 DOM 已渲染
    const timer = setTimeout(setup, 100);

    return () => {
      clearTimeout(timer);
      // 销毁 webview
      invoke("browser_destroy").catch(() => {});
      createdRef.current = false;
      historyStack.current = [];
      historyPos.current = -1;
      // 取消事件监听
      unlistenRef.current.forEach((fn) => fn());
      unlistenRef.current = [];
      // 断开 ResizeObserver
      obs?.disconnect();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  /** 导航到 URL */
  const navigateTo = useCallback(async (url: string) => {
    const normalized = normalizeUrl(url);
    setCurrentUrl(normalized);
    setInputValue(normalized);
    setLoading(true);
    try {
      await invoke("browser_navigate", { url: normalized });
    } catch (e) {
      console.error("[Browser] 导航失败:", e);
      setLoading(false);
    }
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
    setLoading(true);
    invoke("browser_go_back").catch((e) => console.error("[Browser] 后退失败:", e));
    // 手动更新历史位置
    if (historyPos.current > 0) {
      historyPos.current--;
      setCanGoBack(historyPos.current > 0);
      setCanGoForward(true);
    }
  };

  /** 前进 */
  const goForward = () => {
    setLoading(true);
    invoke("browser_go_forward").catch((e) => console.error("[Browser] 前进失败:", e));
    // 手动更新历史位置
    const stack = historyStack.current;
    if (historyPos.current < stack.length - 1) {
      historyPos.current++;
      setCanGoForward(historyPos.current < stack.length - 1);
      setCanGoBack(true);
    }
  };

  /** 刷新 */
  const reload = () => {
    setLoading(true);
    invoke("browser_reload").catch((e) => console.error("[Browser] 刷新失败:", e));
  };

  /** 主页 */
  const goHome = () => navigateTo(DEFAULT_HOME);

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

      {/* 内容区 — webview 将覆盖在此区域之上 */}
      <div className="browser-content" ref={contentRef}>
        {/* 加载指示器 */}
        {loading && (
          <div className="browser-loading-bar">
            <div className="browser-loading-bar-inner" />
          </div>
        )}
        {/* 透明占位：webview 是通过 Tauri 原生层覆盖的，这里只是一个占位容器 */}
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
