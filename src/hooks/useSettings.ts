import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { QuickLink } from "../actions";

/** Settings as reported by the Rust backend. */
export interface Settings {
  hotkey: string;
  hotkey_registered: boolean;
  theme: string;
  language: string;
  search_engine: string;
  quick_links: QuickLink[];
  include_folders: string[];
  exclude_folders: string[];
  launch_at_login: boolean;
  version: string;
  /** Installed from the Microsoft Store, which then handles updates. */
  store_edition: boolean;
}

const DEFAULTS: Settings = {
  hotkey: "Ctrl+Space",
  hotkey_registered: true,
  theme: "system",
  language: "auto",
  search_engine: "google",
  quick_links: [],
  include_folders: [],
  exclude_folders: [],
  launch_at_login: false,
  version: "",
  store_edition: false,
};

/** Make a shortcut such as "Ctrl+KeyK" readable: "Ctrl+K". */
export function formatHotkey(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => part.replace(/^(Key|Digit)(?=.$)/, ""))
    .join("+");
}

/** Apply the theme setting to the document, following the OS when set to "system". */
function useTheme(theme: string) {
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved =
        theme === "system" ? (media.matches ? "light" : "dark") : theme;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

/**
 * Loads settings from the backend and exposes setters that persist changes.
 * Setters reject with the backend's error message if a change is refused.
 */
export function useSettings() {
  const [settings, setSettings] = useState<Settings>(DEFAULTS);

  const refresh = useCallback(async () => {
    try {
      setSettings(await invoke<Settings>("get_settings"));
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useTheme(settings.theme);

  const setHotkey = useCallback(
    async (hotkey: string) => {
      await invoke("set_hotkey", { hotkey });
      await refresh();
    },
    [refresh],
  );

  const setLaunchAtLogin = useCallback(
    async (enabled: boolean) => {
      await invoke("set_launch_at_login", { enabled });
      await refresh();
    },
    [refresh],
  );

  const setAppearance = useCallback(
    async (theme: string, language: string) => {
      setSettings((current) => ({ ...current, theme, language }));
      await invoke("set_appearance", { theme, language });
    },
    [],
  );

  const setSearchEngine = useCallback(async (engine: string) => {
    setSettings((current) => ({ ...current, search_engine: engine }));
    await invoke("set_search_engine", { engine });
  }, []);

  const setQuickLinks = useCallback(
    async (links: QuickLink[]) => {
      await invoke("set_quick_links", { links });
      // The backend drops incomplete rows; show what was actually kept
      await refresh();
    },
    [refresh],
  );

  const setIndexFolders = useCallback(
    async (include: string[], exclude: string[]) => {
      await invoke("set_index_folders", { include, exclude });
      await refresh();
    },
    [refresh],
  );

  return {
    settings,
    setHotkey,
    setLaunchAtLogin,
    setAppearance,
    setSearchEngine,
    setQuickLinks,
    setIndexFolders,
  };
}
