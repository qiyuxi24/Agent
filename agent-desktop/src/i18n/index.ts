import i18n from "i18next";
import { initReactI18next } from "react-i18next";

/**
 * 检测初始语言：localStorage → 浏览器语言 → fallback
 * - Tauri 桌面端 appStore 会将 language 同步到 localStorage
 * - Web 测试环境以 localStorage 为准
 * - 都失败则用浏览器 navigator.language
 */
function detectLanguage(): string {
  try {
    const stored = localStorage.getItem("app-language");
    if (stored === "zh-CN" || stored === "en") return stored;
  } catch {
    // localStorage 不可用
  }
  if (typeof navigator !== "undefined" && navigator.language?.startsWith("zh")) {
    return "zh-CN";
  }
  return "en";
}

const lang = detectLanguage();

// 立即初始化 i18next（同步），避免首屏闪烁
i18n.use(initReactI18next).init({
  lng: lang,
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

// 动态按需加载当前语言包（~6KB，极快）
import(`./locales/${lang}.json`).then((mod) => {
  i18n.addResourceBundle(lang, "translation", mod.default, true, true);
}).catch(() => {
  console.warn(`[i18n] 无法加载语言包: ${lang}`);
});

// 后台预加载另一种语言（语言切换时秒开）
const other = lang === "zh-CN" ? "en" : "zh-CN";
import(`./locales/${other}.json`).then((mod) => {
  i18n.addResourceBundle(other, "translation", mod.default, true, true);
}).catch(() => {
  // 预加载失败不影响当前语言使用
});

export default i18n;
