import React, { useRef, useEffect } from "react";
import type { Answer, SearchResult } from "../hooks/useSearch";
import ResultItem from "./ResultItem";
import { greetingKey, useT, type MessageKey } from "../i18n";

interface ResultsListProps {
  /** Search results, or the user's most-used items when nothing is typed. */
  results: SearchResult[];
  /** A calculation or conversion answered directly, if the query is one. */
  answer: Answer | null;
  onCopyAnswer: () => void;
  query: string;
  /** The launcher shortcut, formatted for display. */
  hotkey: string;
  selectedIndex: number;
  onSelect: (index: number) => void;
  onHover: (index: number) => void;
  isLoading: boolean;
}

/** Results come in groups; the ones beyond plain name matches get a heading. */
const GROUP_TITLE: Record<string, MessageKey> = {
  window: "openWindows",
  content: "insideFiles",
  web: "web",
};

const ResultsList: React.FC<ResultsListProps> = ({
  results,
  answer,
  onCopyAnswer,
  query,
  hotkey,
  selectedIndex,
  onSelect,
  onHover,
}) => {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);
  const isHome = !query.trim();

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

  // The hint text is translated as a whole; the shortcut is rendered as a key cap.
  const [beforeHotkey, afterHotkey] = t("toggleHint").split("{hotkey}");

  return (
    <div className="results-container" ref={containerRef} role="listbox">
      {/* Home: a greeting and the things the user comes back to */}
      {isHome && (
        <div className="home">
          <div className="home-greeting">{t(greetingKey(new Date().getHours()))}</div>
          <div className="home-sub">
            {results.length > 0 ? t("pickUp") : t("freshStart")}
          </div>
          {results.length === 0 && (
            <div className="home-hint">
              {beforeHotkey}
              <kbd>{hotkey}</kbd>
              {afterHotkey}
            </div>
          )}
        </div>
      )}

      {/* The answer, when the query is a sum or a conversion. Click to copy. */}
      {!isHome && answer && (
        <button className="math-result" onClick={onCopyAnswer} tabIndex={-1} title={t("actCopy")}>
          <span className="equals">=</span>
          <span className="value">{answer.value}</span>
          <span className="label">{answer.detail || t("calculator")}</span>
        </button>
      )}

      {results.map((result, idx) => {
        const group = GROUP_TITLE[result.match_type];
        const startsGroup = group && results[idx - 1]?.match_type !== result.match_type;
        return (
          <div
            key={`${result.match_type}:${result.id}`}
            ref={idx === selectedIndex ? selectedRef : undefined}
          >
            {startsGroup && <div className="group-title">{t(group)}</div>}
            <ResultItem
              result={result}
              index={idx}
              isSelected={idx === selectedIndex}
              onSelect={onSelect}
              onHover={onHover}
            />
          </div>
        );
      })}
    </div>
  );
};

export default ResultsList;
