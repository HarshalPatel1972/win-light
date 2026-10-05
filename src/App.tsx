import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SearchInput from "./components/SearchInput";
import ResultsList from "./components/ResultsList";
import PreviewPane from "./components/PreviewPane";
import Settings from "./components/Settings";
import Welcome, { hasBeenWelcomed } from "./components/Welcome";
import { useSearch, type SearchResult } from "./hooks/useSearch";
import { useKeyboardNav } from "./hooks/useKeyboardNav";
import { formatHotkey, useSettings } from "./hooks/useSettings";
import {
  ACTION_KEYS,
  ACTION_LABEL,
  actionFromKey,
  actionsFor,
  isWebItem,
  isWindowItem,
  matchQuickLink,
  needsConfirmation,
  quickLinkItem,
  webItems,
  type ActionId,
} from "./actions";
import { I18nContext, LanguageContext, makeTranslate, resolveLanguage } from "./i18n";

/** How long a confirmation such as "Copied" stays in the status bar. */
const NOTICE_MS = 1600;

function App() {
  const { query, setQuery, results, contentResults, windowResults, answer, isLoading, clearSearch } =
    useSearch(50);
  const {
    settings,
    setHotkey,
    setLaunchAtLogin,
    setAppearance,
    setSearchEngine,
    setQuickLinks,
    setIndexFolders,
  } = useSettings();
  // A command that cannot be undone waits here for a second Enter
  const [awaitingConfirm, setAwaitingConfirm] = useState<string | null>(null);
  const [view, setView] = useState<"search" | "settings">("search");
  const [welcomed, setWelcomed] = useState(hasBeenWelcomed);
  const [suggestions, setSuggestions] = useState<SearchResult[]>([]);
  const [indexCount, setIndexCount] = useState<number>(0);
  const [isIndexing, setIsIndexing] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<string | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);

  const language = resolveLanguage(settings.language);
  const t = useMemo(() => makeTranslate(language), [language]);
  const hotkey = formatHotkey(settings.hotkey);

  // Before anything is typed, the list shows what the user comes back to most.
  // While searching it runs from the closest matches outwards: names, then
  // documents that mention the words, then the web.
  const isHome = !query.trim();
  const items = useMemo(() => {
    if (isHome) return suggestions;
    const named = new Set(results.map((r) => r.filepath.toLowerCase()));
    const inside = contentResults.filter((r) => !named.has(r.filepath.toLowerCase()));
    const web = webItems(query, t("searchWebFor", { query: query.trim() }));
    // A typed web address is unambiguous, so it goes first; the web search
    // is the fallback, so it goes last
    const address = web.filter((item) => item.filepath);
    const webSearch = web.filter((item) => !item.filepath);
    // "yt lofi": a keyword shortcut says exactly where the search should go
    const quick = matchQuickLink(query, settings.quick_links);
    const shortcut = quick
      ? [quickLinkItem(quick.link, t("searchSiteFor", { site: quick.link.name, query: quick.rest }))]
      : [];
    // Something already open is usually what is wanted, so windows lead
    return [...shortcut, ...address, ...windowResults, ...results, ...inside, ...webSearch];
  }, [isHome, suggestions, results, contentResults, windowResults, query, settings.quick_links, t]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  // A launch error only describes the results currently on screen
  useEffect(() => {
    setLaunchError(null);
    setAwaitingConfirm(null);
  }, [query]);

  const showNotice = useCallback((text: string) => {
    setNotice(text);
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), NOTICE_MS);
  }, []);

  const refreshSuggestions = useCallback(() => {
    invoke<SearchResult[]>("get_suggestions").then(setSuggestions).catch(console.error);
  }, []);

  /** The launcher has done its job: get out of the way. */
  const dismiss = useCallback(async () => {
    await getCurrentWindow().hide();
    clearSearch();
    refreshSuggestions();
  }, [clearSearch, refreshSuggestions]);

  // Do something with a result: open it, reveal it, copy its path...
  const runAction = useCallback(
    async (action: ActionId, item: SearchResult | undefined) => {
      if (!item || !actionsFor(item).includes(action)) return;

      try {
        if (item.match_type.startsWith("quick:")) {
          const quick = matchQuickLink(query, settings.quick_links);
          if (quick) {
            await invoke("open_quick_link", { keyword: quick.link.keyword, query: quick.rest });
          }
          await dismiss();
          return;
        }

        if (isWebItem(item)) {
          if (item.filepath) await invoke("open_url", { url: item.filepath });
          else await invoke("web_search", { query });
          await dismiss();
          return;
        }

        if (isWindowItem(item)) {
          // We hold the foreground right now, which is what lets us hand it on
          await invoke("activate_window", { handle: item.id });
          await dismiss();
          return;
        }

        // Restart, shut down and the like run on the second Enter
        if (needsConfirmation(item) && awaitingConfirm !== item.filepath) {
          setAwaitingConfirm(item.filepath);
          showNotice(t("confirmAgain", { name: item.filename }));
          return;
        }
        setAwaitingConfirm(null);

        switch (action) {
          case "copyPath":
            await invoke("copy_text", { text: item.filepath });
            showNotice(t("copied"));
            return;
          case "reveal":
            await invoke("open_containing_folder", { filepath: item.filepath });
            break;
          case "admin":
            await invoke("launch_file", { filepath: item.filepath, mode: "admin", query });
            break;
          case "openWith":
            await invoke("launch_file", { filepath: item.filepath, mode: "open_with", query });
            break;
          default:
            // Passing what was typed lets the app learn: these letters meant this item
            await invoke("launch_file", { filepath: item.filepath, query });
        }
        await dismiss();
      } catch (error) {
        console.error("Action error:", error);
        setLaunchError(String(error));
      }
    },
    [query, dismiss, showNotice, t, settings.quick_links, awaitingConfirm],
  );

  const handleSelect = useCallback(
    (index: number) => runAction("open", items[index]),
    [items, runAction],
  );

  // Handle Escape: hide window and clear search
  const handleEscape = useCallback(async () => {
    clearSearch();
    try {
      const win = getCurrentWindow();
      await win.hide();
    } catch {
      // ignore if window ops fail
    }
  }, [clearSearch]);

  const { selectedIndex, setSelectedIndex, handleKeyDown } = useKeyboardNav(
    items.length,
    handleSelect,
    handleEscape,
    query,
  );
  const selected = items[selectedIndex];

  const handleSearchKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.ctrlKey && e.key === ",") {
        e.preventDefault();
        setView("settings");
        return;
      }
      const action = actionFromKey(e);
      if (action) {
        e.preventDefault();
        runAction(action, selected);
        return;
      }
      handleKeyDown(e);
    },
    [handleKeyDown, runAction, selected],
  );

  const copyAnswer = useCallback(async () => {
    if (!answer) return;
    try {
      await invoke("copy_text", { text: answer.value });
      showNotice(t("copied"));
    } catch (error) {
      setLaunchError(String(error));
    }
  }, [answer, showNotice, t]);

  const installUpdate = useCallback(async () => {
    setIsInstalling(true);
    try {
      await invoke("install_update");
    } catch (error) {
      console.error("Update error:", error);
      setLaunchError(String(error));
      setIsInstalling(false);
    }
  }, []);

  // Listen for backend events
  useEffect(() => {
    const refreshCount = () =>
      invoke<number>("get_index_count").then(setIndexCount).catch(console.error);

    const subscriptions = [
      listen("indexing-started", () => setIsIndexing(true)),
      listen("indexing-complete", () => {
        setIsIndexing(false);
        refreshCount();
        refreshSuggestions();
      }),
      listen("index-updated", refreshCount),
      // The hotkey always lands on the search view
      listen("focus-search", () => {
        setView("search");
        refreshSuggestions();
      }),
      listen("open-settings", () => setView("settings")),
      listen<string>("update-available", (event) => setAvailableUpdate(event.payload)),
    ];

    refreshCount();
    refreshSuggestions();

    // Check if indexing is in progress
    invoke<boolean>("is_indexing").then(setIsIndexing).catch(console.error);

    invoke<string | null>("get_available_update")
      .then(setAvailableUpdate)
      .catch(console.error);

    return () => {
      subscriptions.forEach((subscription) => subscription.then((unlisten) => unlisten()));
    };
  }, [refreshSuggestions]);

  // The footer names the shortcuts for the selected item; three fit comfortably
  const footerActions = selected
    ? actionsFor(selected).filter((action) => action !== "openWith").slice(0, 3)
    : [];

  // A closer look at the selected result, once the user is searching
  const previewed =
    !isHome &&
    selected &&
    !isWebItem(selected) &&
    !isWindowItem(selected) &&
    selected.file_type !== "command"
      ? selected
      : null;

  let content;
  if (!welcomed) {
    content = <Welcome hotkey={hotkey} onDone={() => setWelcomed(true)} />;
  } else if (view === "settings") {
    content = (
      <Settings
        settings={settings}
        setHotkey={setHotkey}
        setLaunchAtLogin={setLaunchAtLogin}
        setAppearance={setAppearance}
        setSearchEngine={setSearchEngine}
        setQuickLinks={setQuickLinks}
        setIndexFolders={setIndexFolders}
        indexCount={indexCount}
        isIndexing={isIndexing}
        availableUpdate={availableUpdate}
        isInstalling={isInstalling}
        onInstallUpdate={installUpdate}
        onClose={() => setView("search")}
      />
    );
  } else {
    content = (
      <>
        <SearchInput
          query={query}
          onQueryChange={setQuery}
          onClear={clearSearch}
          onKeyDown={handleSearchKeyDown}
          isLoading={isLoading}
          activeResultId={items.length > 0 ? `result-${selectedIndex}` : undefined}
        />

        {/* A line of light under the search bar; it travels while indexing */}
        <div className={`beam-line ${isIndexing ? "active" : ""}`} />

        {/* Status bar */}
        <div className="status-bar" role="status" aria-live="polite">
          {launchError ? (
            <span className="error">{launchError}</span>
          ) : notice ? (
            <span className="notice">{notice}</span>
          ) : !settings.hotkey_registered ? (
            <span className="error">{t("hotkeyUnavailable", { hotkey })}</span>
          ) : (
            <span>
              {isIndexing
                ? t("indexing")
                : indexCount > 0
                  ? t("itemsIndexed", { count: indexCount.toLocaleString() })
                  : ""}
            </span>
          )}
          {availableUpdate && (
            <button
              className="update-chip"
              disabled={isInstalling}
              onClick={installUpdate}
              tabIndex={-1}
            >
              {isInstalling
                ? t("installing")
                : `${t("updateAvailable", { version: availableUpdate })} · ${t("install")}`}
            </button>
          )}
        </div>

        <div className={`body ${previewed ? "with-preview" : ""}`}>
          <ResultsList
            results={items}
            answer={answer}
            onCopyAnswer={copyAnswer}
            query={query}
            hotkey={hotkey}
            selectedIndex={selectedIndex}
            onSelect={handleSelect}
            onHover={setSelectedIndex}
            isLoading={isLoading}
          />
          {previewed && (
            <PreviewPane item={previewed} onAction={(action) => runAction(action, previewed)} />
          )}
        </div>

        {/* Footer: what the keys do for the selected item, wherever it is shown */}
        <div className="footer">
          <span>
            <kbd>↑↓</kbd> {t("navigate")}
          </span>
          {selected && footerActions.length > 1 ? (
            footerActions.map((action) => (
              <span key={action}>
                <kbd>{ACTION_KEYS[action]}</kbd> {t(ACTION_LABEL[action])}
              </span>
            ))
          ) : (
            <>
              <span>
                <kbd>Enter</kbd> {t("open")}
              </span>
              <span>
                <kbd>Esc</kbd> {t("close")}
              </span>
              <span>
                <kbd>Ctrl+1-9</kbd> {t("quickLaunch")}
              </span>
            </>
          )}
          <button
            className="footer-button"
            onClick={() => setView("settings")}
            tabIndex={-1}
            title="Ctrl+,"
          >
            ⚙ {t("settings")}
          </button>
        </div>
      </>
    );
  }

  return (
    <I18nContext.Provider value={t}>
      <LanguageContext.Provider value={language}>
        <div className="app-container">{content}</div>
      </LanguageContext.Provider>
    </I18nContext.Provider>
  );
}

export default App;
