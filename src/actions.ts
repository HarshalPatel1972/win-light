import type { SearchResult } from "./hooks/useSearch";
import type { MessageKey } from "./i18n";

/** Store apps are launched through the shell and have no path on disk. */
export const STORE_APP_PREFIX = "shell:AppsFolder\\";

/** What can be done with a result. */
export type ActionId = "open" | "reveal" | "copyPath" | "admin" | "openWith";

export const ACTION_LABEL: Record<ActionId, MessageKey> = {
  open: "actOpen",
  reveal: "actReveal",
  copyPath: "actCopyPath",
  admin: "actRunAdmin",
  openWith: "actOpenWith",
};

/** The keys that trigger each action from the search box. */
export const ACTION_KEYS: Record<ActionId, string> = {
  open: "Enter",
  reveal: "Ctrl+Enter",
  copyPath: "Ctrl+Shift+C",
  admin: "Ctrl+Shift+Enter",
  openWith: "Ctrl+O",
};

const KNOWN_TYPES = ["app", "shortcut", "folder", "document", "image", "code", "web"];

/** The translation key (and colour) for a result's kind; unknown kinds are plain files. */
export function typeKey(fileType: string): MessageKey {
  return `type.${KNOWN_TYPES.includes(fileType) ? fileType : "other"}` as MessageKey;
}

export function isWebItem(item: SearchResult): boolean {
  return item.file_type === "web";
}

/** The actions that make sense for `item`, most common first. */
export function actionsFor(item: SearchResult): ActionId[] {
  if (isWebItem(item) || item.filepath.startsWith(STORE_APP_PREFIX)) return ["open"];
  const runnable = item.file_type === "app" || item.file_type === "shortcut";
  if (runnable) return ["open", "admin", "reveal", "copyPath"];
  if (item.file_type === "folder") return ["open", "reveal", "copyPath"];
  return ["open", "openWith", "reveal", "copyPath"];
}

/** The action a key press asks for, if any. */
export function actionFromKey(e: {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
}): ActionId | null {
  if (!e.ctrlKey) return null;
  const key = e.key.toLowerCase();
  if (key === "enter") return e.shiftKey ? "admin" : "reveal";
  if (key === "c" && e.shiftKey) return "copyPath";
  if (key === "o" && !e.shiftKey) return "openWith";
  return null;
}

const COMMON_TLDS =
  "com|org|net|io|dev|app|ai|co|in|uk|de|fr|es|it|nl|jp|cn|br|au|ca|us|edu|gov|me|tv|info|xyz";
const ADDRESS = new RegExp(`^([a-z0-9-]+\\.)+(${COMMON_TLDS})(/\\S*)?$`, "i");

/** If `query` is a web address, the full URL to open; otherwise null. */
export function asWebAddress(query: string): string | null {
  const text = query.trim();
  if (/\s/.test(text)) return null;
  if (/^https?:\/\/\S+\.\S+/i.test(text)) return text;
  return ADDRESS.test(text) ? `https://${text}` : null;
}

function webItem(id: number, filename: string, filepath: string): SearchResult {
  return {
    id,
    filename,
    filepath,
    extension: "",
    file_size: 0,
    modified_at: 0,
    file_type: "web",
    click_count: 0,
    last_accessed: 0,
    score: 0,
    match_type: "web",
    matched_indices: [],
    snippet: "",
  };
}

/**
 * The rows that take the query to the web: the address itself if one was
 * typed, and always a web search, so the list is never a dead end.
 */
export function webItems(query: string, searchLabel: string): SearchResult[] {
  const address = asWebAddress(query);
  const rows = [webItem(-1001, searchLabel, "")];
  if (address) rows.unshift(webItem(-1000, address.replace(/^https?:\/\//i, ""), address));
  return rows;
}
