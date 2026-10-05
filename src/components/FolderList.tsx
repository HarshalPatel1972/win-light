import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";

interface FolderListProps {
  /** Used to give each control a stable id. */
  name: string;
  folders: string[];
  onChange: (folders: string[]) => void;
}

/** A list of folders with an "Add folder…" button that opens the Windows folder picker. */
const FolderList: React.FC<FolderListProps> = ({ name, folders, onChange }) => {
  const t = useT();

  const add = async () => {
    try {
      const picked = await invoke<string | null>("pick_folder");
      if (picked && !folders.some((f) => f.toLowerCase() === picked.toLowerCase())) {
        onChange([...folders, picked]);
      }
    } catch (error) {
      console.error("Folder picker error:", error);
    }
  };

  return (
    <div className="folder-list">
      {folders.map((folder) => (
        <div className="folder" key={folder}>
          <span className="folder-path" title={folder}>
            {folder}
          </span>
          <button
            className="button remove"
            onClick={() => onChange(folders.filter((f) => f !== folder))}
            aria-label={`${t("remove")}: ${folder}`}
            title={t("remove")}
          >
            ✕
          </button>
        </div>
      ))}
      <button id={`add-${name}`} className="button" onClick={add}>
        + {t("addFolder")}
      </button>
    </div>
  );
};

export default FolderList;
