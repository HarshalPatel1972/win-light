import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Icons already fetched this session (null = the file has no icon). */
const loaded = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();
const MAX_CACHED = 500;

function fetchIcon(filepath: string): Promise<string | null> {
  let request = pending.get(filepath);
  if (!request) {
    request = invoke<string | null>("get_icon", { filepath })
      .catch(() => null)
      .then((icon) => {
        if (loaded.size >= MAX_CACHED) loaded.clear();
        loaded.set(filepath, icon);
        pending.delete(filepath);
        return icon;
      });
    pending.set(filepath, request);
  }
  return request;
}

/**
 * The shell icon of a file as a data URL, or null while it loads
 * (or if the file has none).
 */
export function useIcon(filepath: string): string | null {
  const [icon, setIcon] = useState<string | null>(loaded.get(filepath) ?? null);

  useEffect(() => {
    // Nothing to look up (e.g. a web result)
    if (!filepath) {
      setIcon(null);
      return;
    }
    const cached = loaded.get(filepath);
    if (cached !== undefined) {
      setIcon(cached);
      return;
    }

    let current = true;
    setIcon(null);
    fetchIcon(filepath).then((result) => {
      if (current) setIcon(result);
    });
    return () => {
      current = false;
    };
  }, [filepath]);

  return icon;
}
