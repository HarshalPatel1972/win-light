import { useState, useCallback, useEffect } from "react";

/**
 * Keyboard navigation hook for the results list.
 * Handles ↑/↓ arrows, Enter, Esc, Tab, Ctrl+1-9 quick-launch.
 *
 * The selection returns to the top whenever `resetKey` (the query) changes,
 * but stays put when more results merely arrive for the same query.
 */
export function useKeyboardNav(
  resultCount: number,
  onSelect: (index: number) => void,
  onEscape: () => void,
  resetKey: string,
) {
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    setSelectedIndex(0);
  }, [resetKey]);

  // Never point past the end of a list that got shorter
  useEffect(() => {
    setSelectedIndex((current) => Math.min(current, Math.max(resultCount - 1, 0)));
  }, [resultCount]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent | KeyboardEvent) => {
      // Ctrl+1-9: quick launch
      if (e.ctrlKey && e.key >= "1" && e.key <= "9") {
        e.preventDefault();
        const idx = parseInt(e.key) - 1;
        if (idx < resultCount) {
          onSelect(idx);
        }
        return;
      }

      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((prev) => (prev + 1) % Math.max(resultCount, 1));
          break;

        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((prev) =>
            prev <= 0 ? Math.max(resultCount - 1, 0) : prev - 1,
          );
          break;

        case "Tab":
          e.preventDefault();
          if (e.shiftKey) {
            setSelectedIndex((prev) =>
              prev <= 0 ? Math.max(resultCount - 1, 0) : prev - 1,
            );
          } else {
            setSelectedIndex((prev) => (prev + 1) % Math.max(resultCount, 1));
          }
          break;

        case "Enter":
          e.preventDefault();
          if (resultCount > 0) {
            onSelect(selectedIndex);
          }
          break;

        case "Escape":
          e.preventDefault();
          onEscape();
          break;

        default:
          break;
      }
    },
    [resultCount, selectedIndex, onSelect, onEscape],
  );

  return {
    selectedIndex,
    setSelectedIndex,
    handleKeyDown,
  };
}
