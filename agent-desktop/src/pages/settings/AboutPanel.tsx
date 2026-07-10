import { useTranslation } from "react-i18next";

export default function AboutPanel() {
  const { t } = useTranslation();
  return (
    <section className="settings-panel">
      <div className="about-info">
        <p>{t("settings.about.version")}</p>
        <p className="form-hint">{t("settings.about.tech")}</p>
      </div>
    </section>
  );
}
