import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type ShortcutAction, DEFAULT_SHORTCUTS } from "../../stores/appStore";
import { useKeyCapture, formatCombo } from "../../hooks/useKeyCapture";
import { ResetIcon } from "../../components/Icons";

export default function ShortcutsPanel() {
  const { t } = useTranslation();
  const shortcuts = useAppStore((s) => s.shortcuts);
  const setShortcut = useAppStore((s) => s.setShortcut);

  const [capturingAction, setCapturingAction] = useState<ShortcutAction | null>(null);

  const handleCapture = useCallback(
    (keys: string[]) => {
      if (capturingAction) {
        setShortcut(capturingAction, keys);
        setCapturingAction(null);
      }
    },
    [capturingAction, setShortcut],
  );

  const capture = useKeyCapture(handleCapture);

  useEffect(() => {
    if (capture.listening) {
      document.addEventListener("keydown", capture.handleKeyDown, true);
      document.addEventListener("keyup", capture.handleKeyUp, true);
      return () => {
        document.removeEventListener("keydown", capture.handleKeyDown, true);
        document.removeEventListener("keyup", capture.handleKeyUp, true);
      };
    }
  }, [capture.listening, capture.handleKeyDown, capture.handleKeyUp]);

  const startRebind = (action: ShortcutAction) => {
    setCapturingAction(action);
    capture.startCapture();
  };

  return (
    <section className="settings-panel">
      <div className="shortcut-list">
        {(Object.keys(shortcuts) as ShortcutAction[]).map((action) => {
          const binding = shortcuts[action];
          const isCapturing = capturingAction === action && capture.listening;
          const displayKeys = isCapturing ? capture.currentKeys : binding.keys;

          return (
            <div
              key={action}
              className={`shortcut-item ${isCapturing ? "shortcut-recording" : ""}`}
              onClick={() => {
                if (!capture.listening) startRebind(action);
              }}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  if (!capture.listening) startRebind(action);
                }
              }}
            >
              <span className="shortcut-label">{t(`settings.shortcuts.actions.${action}`)}</span>
              <div className="shortcut-keys">
                {isCapturing ? (
                  <>
                    <span className="shortcut-recording-hint">{t("settings.shortcuts.recording")}</span>
                    {displayKeys.length > 0 && (
                      <kbd className="shortcut-kbd recording">{formatCombo(displayKeys)}</kbd>
                    )}
                  </>
                ) : displayKeys.length > 0 ? (
                  <kbd className="shortcut-kbd">{formatCombo(displayKeys)}</kbd>
                ) : (
                  <span className="shortcut-empty">{t("settings.shortcuts.clickToSet")}</span>
                )}
              </div>
              {!isCapturing && binding.keys.length > 0 && (
                <button
                  className="shortcut-reset-btn"
                  title={t("settings.shortcuts.resetDefault")}
                  onClick={(e) => {
                    e.stopPropagation();
                    setShortcut(action, DEFAULT_SHORTCUTS[action].keys);
                  }}
                >
                  <ResetIcon size={12} />
                </button>
              )}
            </div>
          );
        })}
      </div>
      <p className="form-hint" style={{ marginTop: "12px" }}>
        {t("settings.shortcuts.help")}
      </p>
    </section>
  );
}
