import React, { useCallback, useContext } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SearchResult } from "../hooks/useSearch";
import { useIcon } from "../hooks/useIcon";
import { STORE_APP_PREFIX, isWebItem, typeKey } from "../actions";
import { formatRelativeTime, LanguageContext, useT } from "../i18n";

/** Map file_type to a fallback emoji, shown until the real icon has loaded. */
function getFileIcon(fileType: string, extension: string): string {
  switch (fileType) {
    case "app":
      return "🚀";
    case "shortcut":
      return "🔗";
    case "folder":
      return "📁";
    case "document":
      return getDocIcon(extension);
    case "image":
      return "🖼️";
    case "code":
      return "💻";
    case "web":
      return "🌐";
    default:
      return "📄";
  }
}

function getDocIcon(ext: string): string {
  switch (ext.toLowerCase()) {
    case "pdf":
      return "📕";
    case "doc":
    case "docx":
      return "📘";
    case "xls":
    case "xlsx":
      return "📊";
    case "ppt":
    case "pptx":
      return "📙";
    case "txt":
    case "md":
      return "📝";
    default:
      return "📄";
  }
}

/** Render the filename with matched characters highlighted. */
function highlightName(
  name: string,
  matchedIndices: number[],
): React.ReactNode {
  if (!matchedIndices.length) {
    return name;
  }

  const indexSet = new Set(matchedIndices);
  const parts: React.ReactNode[] = [];
  let currentRun = "";
  let isHighlightRun = false;

  for (let i = 0; i < name.length; i++) {
    const shouldHighlight = indexSet.has(i);

    if (i === 0) {
      isHighlightRun = shouldHighlight;
      currentRun = name[i];
      continue;
    }

    if (shouldHighlight === isHighlightRun) {
      currentRun += name[i];
    } else {
      // Flush current run
      if (isHighlightRun) {
        parts.push(
          <span key={`h-${i}`} className="highlight">
            {currentRun}
          </span>,
        );
      } else {
        parts.push(<span key={`n-${i}`}>{currentRun}</span>);
      }
      currentRun = name[i];
      isHighlightRun = shouldHighlight;
    }
  }

  // Flush last run
  if (currentRun) {
    if (isHighlightRun) {
      parts.push(
        <span key="h-last" className="highlight">
          {currentRun}
        </span>,
      );
    } else {
      parts.push(<span key="n-last">{currentRun}</span>);
    }
  }

  return parts;
}

/** Format file size in human-readable form. */
function formatSize(bytes: number): string {
  if (bytes === 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let idx = 0;
  let size = bytes;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx++;
  }
  return `${size.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

interface ResultItemProps {
  result: SearchResult;
  index: number;
  isSelected: boolean;
  onSelect: (index: number) => void;
  onHover: (index: number) => void;
}

const ResultItem: React.FC<ResultItemProps> = ({
  result,
  index,
  isSelected,
  onSelect,
  onHover,
}) => {
  const t = useT();
  const isWeb = isWebItem(result);
  // Web rows have nothing on disk to take an icon from
  const icon = useIcon(isWeb ? "" : result.filepath);
  const isStoreApp = result.filepath.startsWith(STORE_APP_PREFIX);
  const kind = typeKey(result.file_type);
  const hasFolder = !isStoreApp && !isWeb;
  // What sits under the name: the passage that matched, or where the item lives
  const subtitle = result.snippet || (isStoreApp ? t("installedApp") : result.filepath);
  const language = useContext(LanguageContext);
  // A shortcut stands for the thing it opens; its extension is noise.
  const displayName = result.filename.replace(/\.(lnk|url)$/i, "");

  // The item's history with the user: "Opened 14× · 2 hours ago"
  const usage =
    result.click_count > 0
      ? [
          t("openedTimes", { count: result.click_count }),
          result.last_accessed > 0 && formatRelativeTime(result.last_accessed, language),
        ]
          .filter(Boolean)
          .join(" · ")
      : null;

  const handleContextMenu = useCallback(
    async (e: React.MouseEvent) => {
      e.preventDefault();
      if (!hasFolder) return;
      try {
        await invoke("open_containing_folder", { filepath: result.filepath });
      } catch (err) {
        console.error("Failed to open folder:", err);
      }
    },
    [result.filepath, hasFolder],
  );

  return (
    <div
      className={`result-item ${isSelected ? "selected" : ""}`}
      data-type={kind.slice("type.".length)}
      onClick={() => onSelect(index)}
      onContextMenu={handleContextMenu}
      onMouseEnter={() => onHover(index)}
      role="option"
      aria-selected={isSelected}
      title={hasFolder ? t("openFolderHint") : undefined}
    >
      {/* Icon */}
      <div className="result-icon">
        {icon ? (
          <img src={icon} alt="" draggable={false} />
        ) : (
          getFileIcon(result.file_type, result.extension)
        )}
      </div>

      {/* File info */}
      <div className="result-info">
        <div className="result-name">
          {highlightName(displayName, result.matched_indices)}
        </div>
        {subtitle && (
          <div className="result-path" title={hasFolder ? result.filepath : undefined}>
            {subtitle}
          </div>
        )}
      </div>

      {/* Meta info */}
      <div className="result-meta">
        {usage ? (
          <span className="result-usage">{usage}</span>
        ) : (
          result.file_size > 0 && (
            <span className="result-path result-size">
              {formatSize(result.file_size)}
            </span>
          )
        )}
        <span className={`result-badge ${result.file_type}`}>
          {t(kind)}
        </span>
        {index < 9 && (
          <span className="result-shortcut">⌃{index + 1}</span>
        )}
      </div>
    </div>
  );
};

export default ResultItem;
