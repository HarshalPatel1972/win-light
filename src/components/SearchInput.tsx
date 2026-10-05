import React, { useRef, useEffect } from "react";
import { useT } from "../i18n";

interface SearchInputProps {
  query: string;
  onQueryChange: (q: string) => void;
  onClear: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
  isLoading: boolean;
  /** Element id of the highlighted result, announced by screen readers. */
  activeResultId?: string;
}

/** The search input bar at the top of the launcher. */
const SearchInput: React.FC<SearchInputProps> = ({
  query,
  onQueryChange,
  onClear,
  onKeyDown,
  isLoading,
  activeResultId,
}) => {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-focus on mount and when the window is shown
  useEffect(() => {
    const focusInput = () => {
      inputRef.current?.focus();
      inputRef.current?.select();
    };

    focusInput();

    // Listen for the focus-search event from Rust backend (when window is toggled on)
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen("focus-search", () => {
        focusInput();
      }).then((fn) => {
        unlisten = fn;
      });
    });

    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <div className="search-container" data-tauri-drag-region>
      <div className="search-wrapper">
        {/* The spark: the app's light. It pulses while a search is running. */}
        <div className={`spark ${isLoading ? "thinking" : ""}`} />

        <input
          ref={inputRef}
          className="search-input"
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={t("searchPlaceholder")}
          aria-label={t("searchPlaceholder")}
          role="combobox"
          aria-expanded="true"
          aria-controls="results"
          aria-activedescendant={activeResultId}
          autoFocus
          spellCheck={false}
          autoComplete="off"
        />

        {/* Clear button */}
        {query.length > 0 ? (
          <button
            className="clear-button"
            onClick={onClear}
            tabIndex={-1}
            aria-label={t("clearSearch")}
          >
            ✕
          </button>
        ) : null}
      </div>
    </div>
  );
};

export default SearchInput;
