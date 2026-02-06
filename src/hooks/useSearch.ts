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
  match_type: string;
  matched_indices: number[];
}

/**
 * Custom hook that manages search state:
 * - Debounced query dispatch to Rust backend
 * - Math expression evaluation
 * - Loading state
 */
export function useSearch(debounceMs: number = 50) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [mathResult, setMathResult] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef(0); // generation counter to ignore stale results

  const performSearch = useCallback(async (q: string, generation: number) => {
    if (!q.trim()) {
      setResults([]);
      setMathResult(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);

    try {
      // Run search and math eval in parallel
      const [searchResults, mathEval] = await Promise.all([
        invoke<SearchResult[]>("search", { query: q }),
        invoke<string | null>("eval_math", { query: q }),
      ]);

      // Only update if this is still the latest generation
      if (generation === abortRef.current) {
        setResults(searchResults);
        setMathResult(mathEval);
      }
    } catch (error) {
      console.error("Search error:", error);
      if (generation === abortRef.current) {
        setResults([]);
        setMathResult(null);
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

    if (!query.trim()) {
      setResults([]);
      setMathResult(null);
      setIsLoading(false);
      return;
    }

    const generation = ++abortRef.current;

    timerRef.current = setTimeout(() => {
      performSearch(query, generation);
    }, debounceMs);

    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, [query, debounceMs, performSearch]);

  const clearSearch = useCallback(() => {
    setQuery("");
    setResults([]);
    setMathResult(null);
    setIsLoading(false);
  }, []);

  return {
    query,
    setQuery,
    results,
    mathResult,
    isLoading,
    clearSearch,
  };
}
