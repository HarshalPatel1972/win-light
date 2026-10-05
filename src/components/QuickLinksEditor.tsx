import React, { useEffect, useState } from "react";
import type { QuickLink } from "../actions";
import { useT } from "../i18n";

interface QuickLinksEditorProps {
  links: QuickLink[];
  onSave: (links: QuickLink[]) => Promise<void>;
}

const EMPTY: QuickLink = { keyword: "", name: "", url: "" };

/**
 * Edits the keyword shortcuts ("yt" → YouTube). Changes are saved when a
 * field loses focus, so there is no save button to forget.
 */
const QuickLinksEditor: React.FC<QuickLinksEditorProps> = ({ links, onSave }) => {
  const t = useT();
  const [rows, setRows] = useState<QuickLink[]>(links);

  // Follow what was actually stored (incomplete rows are dropped on save)
  useEffect(() => {
    setRows(links);
  }, [links]);

  const edit = (index: number, field: keyof QuickLink, value: string) =>
    setRows((current) => current.map((row, i) => (i === index ? { ...row, [field]: value } : row)));

  const save = (next: QuickLink[]) => {
    // A row still being typed stays on screen but is not stored yet
    const complete = next.filter((row) => row.keyword.trim() && row.url.trim());
    onSave(complete).catch(console.error);
  };

  const remove = (index: number) => {
    const next = rows.filter((_, i) => i !== index);
    setRows(next);
    save(next);
  };

  return (
    <div className="quick-links">
      {rows.map((row, i) => (
        <div className="quick-link" key={i} onBlur={() => save(rows)}>
          <input
            id={`quick-keyword-${i}`}
            className="field keyword"
            value={row.keyword}
            placeholder={t("keyword")}
            aria-label={t("keyword")}
            spellCheck={false}
            onChange={(e) => edit(i, "keyword", e.target.value)}
          />
          <input
            id={`quick-name-${i}`}
            className="field name"
            value={row.name}
            placeholder={t("siteName")}
            aria-label={t("siteName")}
            spellCheck={false}
            onChange={(e) => edit(i, "name", e.target.value)}
          />
          <input
            id={`quick-url-${i}`}
            className="field url"
            value={row.url}
            placeholder="https://example.com/search?q={query}"
            aria-label="URL"
            spellCheck={false}
            onChange={(e) => edit(i, "url", e.target.value)}
          />
          <button className="button remove" onClick={() => remove(i)} aria-label={t("remove")} title={t("remove")}>
            ✕
          </button>
        </div>
      ))}
      <button className="button" onClick={() => setRows((current) => [...current, EMPTY])}>
        + {t("addShortcut")}
      </button>
    </div>
  );
};

export default QuickLinksEditor;
