import { useTranslation } from "react-i18next";

export default function PluginsPanel() {
  const { t } = useTranslation();
  return (
    <section className="settings-panel">
      <h3 className="panel-title">{t("settings.sections.plugins")}</h3>
      <div className="placeholder-section">
        <p>{t("settings.plugins.comingSoon")}</p>
      </div>
    </section>
  );
}
