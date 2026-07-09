import { useTranslation } from "react-i18next";
import { useAppStore, type ThemeMode } from "../../stores/appStore";

export default function GeneralPanel() {
  const { t } = useTranslation();
  const theme = useAppStore((s) => s.theme);
  const language = useAppStore((s) => s.language);
  const setTheme = useAppStore((s) => s.setTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);

  const themeOptions: { value: ThemeMode; labelKey: string }[] = [
    { value: "system", labelKey: "settings.general.themeSystem" },
    { value: "light", labelKey: "settings.general.themeLight" },
    { value: "dark", labelKey: "settings.general.themeDark" },
  ];

  const langOptions = [
    { value: "zh-CN", label: "简体中文" },
    { value: "en", label: "English" },
  ];

  return (
    <section className="settings-panel">
      <h3 className="panel-title">{t("settings.sections.general")}</h3>
      <div className="form-group">
        <label>{t("settings.general.language")}</label>
        <select value={language} onChange={(e) => setLanguage(e.target.value)} className="form-select">
          {langOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
      </div>
      <div className="form-group">
        <label>{t("settings.general.theme")}</label>
        <div className="theme-selector">
          {themeOptions.map((opt) => (
            <button
              key={opt.value}
              className={`theme-btn ${theme === opt.value ? "active" : ""}`}
              onClick={() => setTheme(opt.value)}
            >
              <span className={`theme-preview theme-preview-${opt.value}`} />
              <span>{t(opt.labelKey)}</span>
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}
