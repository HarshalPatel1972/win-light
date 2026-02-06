import React, { useRef, useEffect } from "react";

interface SearchInputProps {
  query: string;
  onQueryChange: (q: string) => void;
  onClear: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
  isLoading: boolean;
}

/** The search input bar at the top of the launcher. */
const SearchInput: React.FC<SearchInputProps> = ({
  query,
  onQueryChange,
  onClear,
  onKeyDown,
  isLoading,
}) => {
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
        {/* Search icon */}
        <svg
          className="search-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>

        <input
          ref={inputRef}
          className="search-input"
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Search apps and files..."
          autoFocus
          spellCheck={false}
          autoComplete="off"
        />

        {/* Loading spinner or clear button */}
        {isLoading ? (
          <div className="spinner" />
        ) : query.length > 0 ? (
          <button
            className="clear-button"
            onClick={onClear}
            tabIndex={-1}
            aria-label="Clear search"
          >
            ✕
          </button>
        ) : null}
      </div>
    </div>
  );
};

export default SearchInput;
