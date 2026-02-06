import React, { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SearchResult } from "../hooks/useSearch";

/** Map file_type to an emoji icon and badge style. */
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
  const icon = getFileIcon(result.file_type, result.extension);

  const handleContextMenu = useCallback(
    async (e: React.MouseEvent) => {
      e.preventDefault();
      try {
        await invoke("open_containing_folder", { filepath: result.filepath });
      } catch (err) {
        console.error("Failed to open folder:", err);
      }
    },
    [result.filepath],
  );

  return (
    <div
      className={`result-item ${isSelected ? "selected" : ""}`}
      onClick={() => onSelect(index)}
      onContextMenu={handleContextMenu}
      onMouseEnter={() => onHover(index)}
      role="option"
      aria-selected={isSelected}
      title="Right-click to open containing folder"
    >
      {/* Icon */}
      <div className="result-icon">{icon}</div>

      {/* File info */}
      <div className="result-info">
        <div className="result-name">
          {highlightName(result.filename, result.matched_indices)}
        </div>
        <div className="result-path" title={result.filepath}>
          {result.filepath}
        </div>
      </div>

      {/* Meta info */}
      <div className="result-meta">
        {result.file_size > 0 && (
          <span className="result-path" style={{ fontSize: "10px" }}>
            {formatSize(result.file_size)}
          </span>
        )}
        <span className={`result-badge ${result.file_type}`}>
          {result.file_type}
        </span>
        {index < 9 && (
          <span className="result-shortcut">⌃{index + 1}</span>
        )}
      </div>
    </div>
  );
};

export default ResultItem;
