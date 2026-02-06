import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SearchInput from "./components/SearchInput";
import ResultsList from "./components/ResultsList";
import { useSearch } from "./hooks/useSearch";
import { useKeyboardNav } from "./hooks/useKeyboardNav";

function App() {
  const { query, setQuery, results, mathResult, isLoading, clearSearch } =
    useSearch(50);
  const [indexCount, setIndexCount] = useState<number>(0);
  const [isIndexing, setIsIndexing] = useState(false);

  // Launch the selected result
  const handleSelect = useCallback(
    async (index: number) => {
      const result = results[index];
      if (!result) return;

      try {
        await invoke("launch_file", { filepath: result.filepath });
        // Hide window after launching
        const win = getCurrentWindow();
        await win.hide();
        clearSearch();
      } catch (error) {
        console.error("Launch error:", error);
      }
    },
    [results, clearSearch],
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
    results.length,
    handleSelect,
    handleEscape,
  );

  // Listen for backend events
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    listen("indexing-started", () => {
      setIsIndexing(true);
    }).then((fn) => unlisteners.push(fn));

    listen("indexing-complete", () => {
      setIsIndexing(false);
      // Refresh count
      invoke<number>("get_index_count")
        .then(setIndexCount)
        .catch(console.error);
    }).then((fn) => unlisteners.push(fn));

    // Get initial count
    invoke<number>("get_index_count")
      .then(setIndexCount)
      .catch(console.error);

    // Check if indexing is in progress
    invoke<boolean>("is_indexing")
      .then(setIsIndexing)
      .catch(console.error);

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  return (
    <div className="app-container">
      <SearchInput
        query={query}
        onQueryChange={setQuery}
        onClear={clearSearch}
        onKeyDown={handleKeyDown}
        isLoading={isLoading}
      />

      {/* Status bar */}
      <div className="status-bar">
        <span>
          {indexCount > 0 ? `${indexCount.toLocaleString()} files indexed` : ""}
        </span>
        {isIndexing && (
          <span className="indexing">
            <span className="spinner" />
            Indexing...
          </span>
        )}
      </div>

      <ResultsList
        results={results}
        mathResult={mathResult}
        query={query}
        selectedIndex={selectedIndex}
        onSelect={handleSelect}
        onHover={setSelectedIndex}
        isLoading={isLoading}
      />

      {/* Footer with keyboard hints */}
      <div className="footer">
        <span>
          <kbd>↑↓</kbd> Navigate
        </span>
        <span>
          <kbd>Enter</kbd> Open
        </span>
        <span>
          <kbd>Esc</kbd> Close
        </span>
        <span>
          <kbd>Ctrl+1-9</kbd> Quick launch
        </span>
      </div>
    </div>
  );
}

export default App;
