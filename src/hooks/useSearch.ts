import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Shape of a search result from the Rust backend. */
export interface SearchResult {
  id: number;
  filename: string;
  filepath: string;
  extension: string;
  file_size: number;
  modified_at: number;
  file_type: string;
  click_count: number;
  last_accessed: number;
  score: number;
  /** "exact", "prefix", "substring", "fuzzy", "path", "content" or "web" */
  match_type: string;
  matched_indices: number[];
  /** For matches inside a document: the passage that matched. */
  snippet: string;
}

/** A calculation or conversion answered directly. */
export interface Answer {
  value: string;
  /** "calculator", "unit" or "currency" */
  kind: string;
  /** What was asked, normalised (e.g. "5 km"); empty for plain arithmetic. */
  detail: string;
}

/** Searching inside documents is slower than matching names, so it waits for a pause in typing. */
const CONTENT_DEBOUNCE_MS = 220;
const CONTENT_MIN_CHARS = 3;

/**
 * Custom hook that manages search state:
 * - Debounced query dispatch to Rust backend
 * - Instant answers (maths, conversions)
 * - Matches inside documents, which arrive a moment after the name matches
 * - Loading state
 */
export function useSearch(debounceMs: number = 50) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [contentResults, setContentResults] = useState<SearchResult[]>([]);
  const [windowResults, setWindowResults] = useState<SearchResult[]>([]);
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef(0); // generation counter to ignore stale results

  const performSearch = useCallback(async (q: string, generation: number) => {
    setIsLoading(true);

    try {
      // Run search and the answer engine in parallel
      const [searchResults, answered, openWindows] = await Promise.all([
        invoke<SearchResult[]>("search", { query: q }),
        invoke<Answer | null>("eval_math", { query: q }),
        invoke<SearchResult[]>("search_windows", { query: q }).catch(() => []),
      ]);

      // Only update if this is still the latest generation
      if (generation === abortRef.current) {
        setResults(searchResults);
        setAnswer(answered);
        setWindowResults(openWindows);
      }
    } catch (error) {
      console.error("Search error:", error);
      if (generation === abortRef.current) {
        setResults([]);
        setAnswer(null);
      }
    } finally {
      if (generation === abortRef.current) {
        setIsLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
    }

    const generation = ++abortRef.current;
    const trimmed = query.trim();

    if (!trimmed) {
      setResults([]);
      setContentResults([]);
      setWindowResults([]);
      setAnswer(null);
      setIsLoading(false);
      return;
    }

    timerRef.current = setTimeout(() => {
      performSearch(query, generation);
    }, debounceMs);

    // Matches inside documents follow once typing pauses
    setContentResults([]);
    const contentTimer =
      trimmed.length >= CONTENT_MIN_CHARS
        ? setTimeout(() => {
            invoke<SearchResult[]>("search_content", { query })
              .then((found) => {
                if (generation === abortRef.current) setContentResults(found);
              })
              .catch((error) => console.error("Content search error:", error));
          }, CONTENT_DEBOUNCE_MS)
        : null;

    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
      if (contentTimer !== null) {
        clearTimeout(contentTimer);
      }
    };
  }, [query, debounceMs, performSearch]);

  const clearSearch = useCallback(() => {
    setQuery("");
    setResults([]);
    setContentResults([]);
    setWindowResults([]);
    setAnswer(null);
    setIsLoading(false);
  }, []);

  return {
    query,
    setQuery,
    results,
    contentResults,
    windowResults,
    answer,
    isLoading,
    clearSearch,
  };
}
