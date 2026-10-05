import React, { useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SearchResult } from "../hooks/useSearch";
import {
  ACTION_KEYS,
  ACTION_LABEL,
  STORE_APP_PREFIX,
  actionsFor,
  isVirtualPath,
  typeKey,
  type ActionId,
} from "../actions";
import { formatRelativeTime, LanguageContext, useT } from "../i18n";

/** What the backend reads from disk for the selected result. */
interface Preview {
  image: string | null;
  /** Whether the image shows the file's contents rather than its icon. */
  is_thumbnail: boolean;
  size: number;
  modified: number;
  is_folder: boolean;
  /** The opening lines of a text file. */
  text: string | null;
}

/** Wait this long on a result before loading its preview, so arrowing through a list stays instant. */
const PREVIEW_DELAY_MS = 120;

function formatSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let idx = 0;
  let size = bytes;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx++;
  }
  return `${size.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

interface PreviewPaneProps {
  item: SearchResult;
  onAction: (action: ActionId) => void;
}

/** A closer look at the selected result: its picture, its facts, and what you can do with it. */
const PreviewPane: React.FC<PreviewPaneProps> = ({ item, onAction }) => {
  const t = useT();
  const language = useContext(LanguageContext);
  const [preview, setPreview] = useState<Preview | null>(null);

  useEffect(() => {
    setPreview(null);
    let current = true;
    const timer = setTimeout(() => {
      invoke<Preview>("get_preview", { filepath: item.filepath })
        .then((loaded) => {
          if (current) setPreview(loaded);
        })
        .catch(() => {});
    }, PREVIEW_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [item.filepath]);

  const isStoreApp = item.filepath.startsWith(STORE_APP_PREFIX);
  const name = item.filename.replace(/\.(lnk|url)$/i, "");
  const facts: [string, string][] = [];
  if (preview && preview.size > 0) facts.push([t("size"), formatSize(preview.size)]);
  if (preview && preview.modified > 0) {
    const when = new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
      preview.modified * 1000,
    );
    facts.push([t("modified"), when]);
  }
  if (item.click_count > 0) {
    const last = item.last_accessed > 0 ? ` · ${formatRelativeTime(item.last_accessed, language)}` : "";
    facts.push(["", t("openedTimes", { count: item.click_count }) + last]);
  }

  return (
    <aside className="preview" data-type={item.file_type}>
      {/* Text is best previewed as text; everything else as a picture */}
      {preview?.text && !preview.is_thumbnail ? (
        <pre className="preview-text">{preview.text}</pre>
      ) : (
        <div className={`preview-picture ${preview?.is_thumbnail ? "" : "icon"}`}>
          {preview?.image && <img src={preview.image} alt="" draggable={false} />}
        </div>
      )}
      <div className="preview-name">{name}</div>
      <div className="preview-kind">
        {t(typeKey(item.file_type))}
        {item.extension && !/^(lnk|url)$/i.test(item.extension) ? ` · ${item.extension.toUpperCase()}` : ""}
      </div>

      {item.snippet && <p className="preview-snippet">{item.snippet}</p>}

      {facts.length > 0 && (
        <dl className="preview-facts">
          {facts.map(([label, value]) => (
            <React.Fragment key={label + value}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </React.Fragment>
          ))}
        </dl>
      )}

      <div className="preview-path">
        {isStoreApp ? t("installedApp") : isVirtualPath(item.filepath) ? t("systemCommand") : item.filepath}
      </div>

      <div className="preview-actions">
        {actionsFor(item).map((action) => (
          <button key={action} className="preview-action" onClick={() => onAction(action)} tabIndex={-1}>
            <span>{t(ACTION_LABEL[action])}</span>
            <kbd>{ACTION_KEYS[action]}</kbd>
          </button>
        ))}
      </div>
    </aside>
  );
};

export default PreviewPane;
