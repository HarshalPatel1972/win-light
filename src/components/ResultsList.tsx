import React, { useRef, useEffect } from "react";
import type { SearchResult } from "../hooks/useSearch";
import ResultItem from "./ResultItem";

interface ResultsListProps {
  results: SearchResult[];
  mathResult: string | null;
  query: string;
  selectedIndex: number;
  onSelect: (index: number) => void;
  onHover: (index: number) => void;
  isLoading: boolean;
}

const ResultsList: React.FC<ResultsListProps> = ({
  results,
  mathResult,
  query,
  selectedIndex,
  onSelect,
  onHover,
  isLoading,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to keep selected item visible
  useEffect(() => {
    if (selectedRef.current && containerRef.current) {
      const container = containerRef.current;
      const item = selectedRef.current;
      const containerRect = container.getBoundingClientRect();
      const itemRect = item.getBoundingClientRect();

      if (itemRect.bottom > containerRect.bottom) {
        item.scrollIntoView({ block: "nearest", behavior: "smooth" });
      } else if (itemRect.top < containerRect.top) {
        item.scrollIntoView({ block: "nearest", behavior: "smooth" });
      }
    }
  }, [selectedIndex]);

  // Show nothing if no query
  if (!query.trim()) {
    return (
      <div className="results-container">
        <div className="no-results">
          <div className="icon">🔍</div>
          <div className="text">Type to search apps and files</div>
          <div className="text" style={{ fontSize: "12px", opacity: 0.6 }}>
            Press <kbd style={{
              display: "inline-flex",
              padding: "1px 5px",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border-color)",
              borderRadius: "4px",
              fontSize: "10px",
              margin: "0 2px",
            }}>Ctrl+Space</kbd> to toggle this window
          </div>
        </div>
      </div>
    );
  }

  // Loading state
  if (isLoading && results.length === 0) {
    return (
      <div className="results-container">
        <div className="no-results">
          <div className="spinner" style={{ width: 24, height: 24 }} />
          <div className="text">Searching...</div>
        </div>
      </div>
    );
  }

  // No results
  if (!isLoading && results.length === 0 && !mathResult) {
    return (
      <div className="results-container">
        <div className="no-results">
          <div className="icon">🤷</div>
          <div className="text">No results found for "{query}"</div>
        </div>
      </div>
    );
  }

  return (
    <div className="results-container" ref={containerRef} role="listbox">
      {/* Math result */}
      {mathResult && (
        <div className="math-result">
          <span className="equals">=</span>
          <span className="value">{mathResult}</span>
          <span className="label">Calculator</span>
        </div>
      )}

      {/* File results */}
      {results.map((result, idx) => (
        <div
          key={result.id}
          ref={idx === selectedIndex ? selectedRef : undefined}
        >
          <ResultItem
            result={result}
            index={idx}
            isSelected={idx === selectedIndex}
            onSelect={onSelect}
            onHover={onHover}
          />
        </div>
      ))}
    </div>
  );
};

export default ResultsList;
