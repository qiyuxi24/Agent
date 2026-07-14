import { useState } from "react";
import { useTranslation } from "react-i18next";
import GeneralPanel from "./settings/GeneralPanel";
import ModelsPanel from "./settings/ModelsPanel";
import ShortcutsPanel from "./settings/ShortcutsPanel";
import ToolsPanel from "./settings/ToolsPanel";
import SkillsPanel from "./settings/SkillsPanel";
import PluginsPanel from "./settings/PluginsPanel";
import RagPanel from "./settings/RagPanel";
import AboutPanel from "./settings/AboutPanel";
import PetPanel from "./settings/PetPanel";
import {
  SettingsIcon,
  MonitorIcon,
  KeyboardIcon,
  ToolIcon,
  HelpIcon,
  CloseIcon,
  PackageIcon,
  ExtensionIcon,
  DatabaseIcon,
  PawIcon,
} from "../components/Icons";

interface SettingsPageProps {
  onClose?: () => void;
}

type SettingsSection = "general" | "models" | "shortcuts" | "tools" | "skills" | "plugins" | "rag" | "pet" | "about";

export default function SettingsPage({ onClose }: SettingsPageProps) {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SettingsSection>("general");

  const menuItems: { id: SettingsSection; label: string; icon: React.ReactNode }[] = [
    { id: "general",   label: t("settings.sections.general"),   icon: <SettingsIcon size={18} /> },
    { id: "models",    label: t("settings.sections.models"),    icon: <MonitorIcon size={18} /> },
    { id: "shortcuts", label: t("settings.sections.shortcuts"), icon: <KeyboardIcon size={18} /> },
    { id: "tools",     label: t("settings.sections.tools"),     icon: <ToolIcon size={18} /> },
    { id: "skills",    label: t("settings.sections.skills"),    icon: <PackageIcon size={18} /> },
    { id: "plugins",   label: t("settings.sections.plugins"),   icon: <ExtensionIcon size={18} /> },
    { id: "rag",       label: t("settings.sections.rag"),       icon: <DatabaseIcon size={18} /> },
    { id: "pet",       label: t("settings.sections.pet"),       icon: <PawIcon size={18} /> },
    { id: "about",     label: t("settings.sections.about"),     icon: <HelpIcon size={18} /> },
  ];

  const renderPanel = () => {
    switch (activeSection) {
      case "general":   return <GeneralPanel />;
      case "models":    return <ModelsPanel />;
      case "shortcuts": return <ShortcutsPanel />;
      case "tools":     return <ToolsPanel />;
      case "skills":    return <SkillsPanel />;
      case "plugins":   return <PluginsPanel />;
      case "rag":       return <RagPanel />;
      case "pet":       return <PetPanel />;
      case "about":     return <AboutPanel />;
    }
  };

  return (
    <div className="settings-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose?.(); }}>
      <div className="settings-layout">
        <aside className="settings-sidebar">
          <div className="settings-sidebar-header">
            <h2>{t("settings.title")}</h2>
          </div>
          <nav className="settings-nav">
            {menuItems.map((item) => (
              <button
                key={item.id}
                className={`settings-nav-item ${activeSection === item.id ? "active" : ""}`}
                onClick={() => setActiveSection(item.id)}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </aside>

        <div className="settings-content">
          <div className="settings-content-header">
            <h2>{menuItems.find((i) => i.id === activeSection)?.label}</h2>
            {onClose && (
              <button className="btn btn-icon settings-close-btn" onClick={onClose} title={t("settings.close")}>
                <CloseIcon size={20} />
              </button>
            )}
          </div>
          <div className="settings-content-body">
            {renderPanel()}
          </div>
        </div>
      </div>
    </div>
  );
}
