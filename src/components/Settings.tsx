import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LANGUAGES, useT } from "../i18n";
import { formatHotkey, type Settings as SettingsData } from "../hooks/useSettings";

interface SettingsProps {
  settings: SettingsData;
  setHotkey: (hotkey: string) => Promise<void>;
  setLaunchAtLogin: (enabled: boolean) => Promise<void>;
  setAppearance: (theme: string, language: string) => Promise<void>;
  setSearchEngine: (engine: string) => Promise<void>;
  indexCount: number;
  isIndexing: boolean;
  availableUpdate: string | null;
  isInstalling: boolean;
  onInstallUpdate: () => void;
  onClose: () => void;
}

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta"];

/** Build a shortcut string such as "Ctrl+Alt+KeyK" from a key press. */
function hotkeyFromEvent(e: React.KeyboardEvent): string | null {
  const modifiers = [
    e.ctrlKey && "Ctrl",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Super",
  ].filter(Boolean);
  // A bare letter would hijack normal typing everywhere; function keys are fine.
  if (modifiers.length === 0 && !/^F\d+$/.test(e.code)) return null;
  return [...modifiers, e.code].join("+");
}

/** The settings screen, shown in place of the search view. */
const Settings: React.FC<SettingsProps> = ({
  settings,
  setHotkey,
  setLaunchAtLogin,
  setAppearance,
  setSearchEngine,
  indexCount,
  isIndexing,
  availableUpdate,
  isInstalling,
  onInstallUpdate,
  onClose,
}) => {
  const t = useT();
  const [recording, setRecording] = useState(false);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  // Esc closes settings (while recording a shortcut it cancels the recording instead)
  useEffect(() => {
    if (recording) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [recording, onClose]);

  const handleHotkeyKeyDown = (e: React.KeyboardEvent) => {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      setRecording(false);
      return;
    }
    if (MODIFIER_KEYS.includes(e.key)) return;

    const hotkey = hotkeyFromEvent(e);
    if (!hotkey) {
      setHotkeyError(t("shortcutNeedsModifier"));
      return;
    }
    setRecording(false);
    setHotkey(hotkey)
      .then(() => setHotkeyError(null))
      .catch((error) => setHotkeyError(String(error)));
  };

  const checkForUpdate = async () => {
    setChecking(true);
    setUpdateStatus(null);
    try {
      const version = await invoke<string | null>("check_for_update");
      if (!version) setUpdateStatus(t("upToDate"));
    } catch (error) {
      setUpdateStatus(String(error));
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="settings">
      <div className="settings-header" data-tauri-drag-region>
        <button className="button" onClick={onClose}>
          ← {t("back")}
        </button>
        <span className="settings-title">{t("settings")}</span>
        <span className="settings-version">
          {settings.version && t("version", { version: settings.version })}
        </span>
      </div>

      <div className="settings-body">
        <div className="settings-row">
          <div className="settings-label">
            <span>{t("shortcut")}</span>
            <span className={hotkeyError ? "settings-hint error" : "settings-hint"}>
              {hotkeyError ??
                (settings.hotkey_registered
                  ? t("shortcutHelp")
                  : t("hotkeyUnavailable", { hotkey: formatHotkey(settings.hotkey) }))}
            </span>
          </div>
          <button
            className={`button hotkey-button ${recording ? "recording" : ""}`}
            onClick={() => setRecording(true)}
            onKeyDown={handleHotkeyKeyDown}
            onBlur={() => setRecording(false)}
          >
            {recording ? t("pressKeys") : formatHotkey(settings.hotkey)}
          </button>
        </div>

        <div className="settings-row">
          <label className="settings-label" htmlFor="launch-at-login">
            {t("launchAtLogin")}
          </label>
          <input
            id="launch-at-login"
            type="checkbox"
            className="toggle"
            checked={settings.launch_at_login}
            onChange={(e) => setLaunchAtLogin(e.target.checked).catch(console.error)}
          />
        </div>

        <div className="settings-row">
          <label className="settings-label" htmlFor="theme">
            {t("theme")}
          </label>
          <select
            id="theme"
            className="select"
            value={settings.theme}
            onChange={(e) => setAppearance(e.target.value, settings.language)}
          >
            <option value="system">{t("themeSystem")}</option>
            <option value="dark">{t("themeDark")}</option>
            <option value="light">{t("themeLight")}</option>
          </select>
        </div>

        <div className="settings-row">
          <label className="settings-label" htmlFor="language">
            {t("language")}
          </label>
          <select
            id="language"
            className="select"
            value={settings.language}
            onChange={(e) => setAppearance(settings.theme, e.target.value)}
          >
            <option value="auto">{t("languageAuto")}</option>
            {LANGUAGES.map(({ code, name }) => (
              <option key={code} value={code}>
                {name}
              </option>
            ))}
          </select>
        </div>

        <div className="settings-row">
          <label className="settings-label" htmlFor="search-engine">
            {t("webSearchEngine")}
          </label>
          <select
            id="search-engine"
            className="select"
            value={settings.search_engine}
            onChange={(e) => setSearchEngine(e.target.value).catch(console.error)}
          >
            <option value="google">Google</option>
            <option value="bing">Bing</option>
            <option value="duckduckgo">DuckDuckGo</option>
          </select>
        </div>

        <div className="settings-row">
          <div className="settings-label">
            <span>{t("index")}</span>
            <span className="settings-hint">
              {isIndexing
                ? t("indexing")
                : t("itemsIndexed", { count: indexCount.toLocaleString() })}
            </span>
          </div>
          <button
            className="button"
            disabled={isIndexing}
            onClick={() => invoke("rebuild_index").catch(console.error)}
          >
            {t("rebuildIndex")}
          </button>
        </div>

        <div className="settings-row">
          <div className="settings-label">
            <span>{t("updates")}</span>
            <span className="settings-hint">
              {availableUpdate
                ? t("updateAvailable", { version: availableUpdate })
                : updateStatus}
            </span>
          </div>
          {availableUpdate ? (
            <button className="button primary" disabled={isInstalling} onClick={onInstallUpdate}>
              {isInstalling ? t("installing") : t("install")}
            </button>
          ) : (
            <button className="button" disabled={checking} onClick={checkForUpdate}>
              {checking ? t("checking") : t("checkForUpdates")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
};

export default Settings;
