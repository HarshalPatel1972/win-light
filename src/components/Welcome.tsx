import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";

const STORAGE_KEY = "matchstick.welcomed";

/** Whether the first-run welcome has already been shown on this PC. */
export function hasBeenWelcomed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    // No storage available: never nag.
    return true;
  }
}

function rememberWelcomed() {
  try {
    localStorage.setItem(STORAGE_KEY, "1");
  } catch {
    // ignore
  }
}

/**
 * The story, one scene at a time:
 *   0  Dark    – your PC is full of things you cannot see
 *   1  Strike  – a match is struck and lights them up (the name)
 *   2  How     – press the shortcut, type, every result is a "match"
 *   3  Ready   – it burns brighter the more it is used
 */
const LAST_SCENE = 3;
/** How long each scene plays before moving on; the last one waits for the user. */
const SCENE_MS = [4400, 5400, 7600];

/** The things on the PC: faint tiles scattered around the stage. */
const THINGS: { x: number; y: number; hue: string }[] = [
  { x: 9, y: 16, hue: "app" }, { x: 21, y: 60, hue: "document" }, { x: 15, y: 86, hue: "folder" },
  { x: 30, y: 24, hue: "image" }, { x: 36, y: 76, hue: "code" }, { x: 27, y: 44, hue: "shortcut" },
  { x: 43, y: 10, hue: "folder" }, { x: 58, y: 12, hue: "app" }, { x: 64, y: 78, hue: "document" },
  { x: 71, y: 28, hue: "code" }, { x: 74, y: 56, hue: "image" }, { x: 84, y: 14, hue: "shortcut" },
  { x: 88, y: 44, hue: "folder" }, { x: 82, y: 84, hue: "app" }, { x: 93, y: 68, hue: "document" },
  { x: 41, y: 92, hue: "shortcut" }, { x: 6, y: 42, hue: "code" }, { x: 66, y: 46, hue: "app" },
];

/** Where the match burns, in percent of the stage. */
const LIGHT = { x: 50, y: 45 };

/** 0 at the flame, 1 at the far corners: how late and how dimly a tile lights up. */
function distanceFromLight(x: number, y: number): number {
  return Math.min(1, Math.hypot(x - LIGHT.x, (y - LIGHT.y) * 0.75) / 50);
}

/** A real app or file from this PC, with its shell icon. */
interface IntroItem {
  name: string;
  file_type: string;
  icon: string | null;
}

/** What the backend found on this PC to tell the story with. */
interface IntroData {
  things: IntroItem[];
  demo_query: string;
  demo_rows: IntroItem[];
}

/** Stand-in demo, used only if nothing suitable was found on the PC. */
const FALLBACK_QUERY = "pho";
const FALLBACK_ROWS: IntroItem[] = [
  { name: "Photos", file_type: "app", icon: null },
  { name: "photo-album.png", file_type: "image", icon: null },
  { name: "Phone bills.pdf", file_type: "document", icon: null },
];

interface WelcomeProps {
  /** The launcher shortcut, formatted for display. */
  hotkey: string;
  onDone: () => void;
}

/** First-run story: why it is called Matchstick, and how to use it. */
const Welcome: React.FC<WelcomeProps> = ({ hotkey, onDone }) => {
  const t = useT();
  const [scene, setScene] = useState(0);
  // Progress inside the "how" scene: 0 press the keys, 1 type, 2 results
  const [step, setStep] = useState(0);
  const [typed, setTyped] = useState(0);
  const [intro, setIntro] = useState<IntroData | null>(null);

  // The story is told with the user's own apps and files
  useEffect(() => {
    invoke<IntroData>("get_intro").then(setIntro).catch(console.error);
  }, []);

  const hasDemo = !!intro && intro.demo_rows.length > 0;
  const demoQuery = hasDemo ? intro.demo_query : FALLBACK_QUERY;
  const demoRows = hasDemo ? intro.demo_rows : FALLBACK_ROWS;
  const queryLength = demoQuery.length;

  const finish = useCallback(() => {
    rememberWelcomed();
    onDone();
  }, [onDone]);

  const next = useCallback(() => {
    setScene((current) => {
      if (current >= LAST_SCENE) {
        finish();
        return current;
      }
      return current + 1;
    });
  }, [finish]);

  // Scenes advance on their own; the last one waits for the user
  useEffect(() => {
    if (scene >= LAST_SCENE) return;
    const timer = setTimeout(next, SCENE_MS[scene]);
    return () => clearTimeout(timer);
  }, [scene, next]);

  // The "how it works" demo plays itself: keys, then typing, then results
  useEffect(() => {
    setStep(0);
    setTyped(0);
    if (scene !== 2) return;
    const timers = [
      setTimeout(() => setStep(1), 1600),
      ...Array.from({ length: queryLength }, (_, i) =>
        setTimeout(() => setTyped(i + 1), 2100 + i * 260),
      ),
      setTimeout(() => setStep(2), 3200),
    ];
    return () => timers.forEach(clearTimeout);
  }, [scene, queryLength]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        finish();
      } else if (e.key === "Enter" || e.key === " " || e.key === "ArrowRight") {
        e.preventDefault();
        next();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [finish, next]);

  const [beforeHotkey, afterHotkey] = t("introHow1").split("{hotkey}");
  const keys = hotkey.split("+");

  return (
    <div className={`intro scene-${scene} ${scene >= 1 ? "lit" : ""}`} data-tauri-drag-region>
      <div className="intro-stage">
        {THINGS.map(({ x, y, hue }, i) => {
          const distance = distanceFromLight(x, y);
          const icon = intro?.things[i]?.icon;
          return (
            <div
              key={i}
              className={`intro-thing ${icon ? "has-icon" : ""}`}
              style={
                {
                  left: `${x}%`,
                  top: `${y}%`,
                  "--hue": `var(--hue-${hue})`,
                  "--lit": (0.9 - 0.65 * distance).toFixed(2),
                  "--delay": `${(distance * 0.9).toFixed(2)}s`,
                } as React.CSSProperties
              }
            >
              {icon && <img src={icon} alt="" draggable={false} />}
            </div>
          );
        })}

        <div className="intro-light" />

        {/* The match */}
        <div className="intro-match">
          <svg viewBox="0 0 60 200" width="60" height="200" aria-hidden="true">
            <defs>
              <linearGradient id="intro-wood" x1="0" x2="1">
                <stop offset="0" stopColor="#c99a66" />
                <stop offset="0.5" stopColor="#ecc999" />
                <stop offset="1" stopColor="#b98952" />
              </linearGradient>
              <radialGradient id="intro-flame" cx="0.5" cy="0.75" r="0.75">
                <stop offset="0" stopColor="#fffbe8" />
                <stop offset="0.35" stopColor="#ffd27a" />
                <stop offset="0.75" stopColor="#ff8f4b" />
                <stop offset="1" stopColor="#ff5a36" stopOpacity="0.85" />
              </radialGradient>
            </defs>
            <rect x="26.5" y="58" width="7" height="140" rx="3" fill="url(#intro-wood)" />
            <ellipse className="intro-head" cx="30" cy="56" rx="8" ry="11" />
            <g className="intro-flame">
              <path
                className="intro-flame-shape"
                d="M30 2 C40 20 47 30 44 44 C42 54 36 60 30 60 C24 60 18 54 16 44 C13 30 22 22 30 2 Z"
                fill="url(#intro-flame)"
              />
            </g>
          </svg>
          {[0, 1, 2, 3, 4, 5, 6].map((i) => (
            <span
              key={i}
              className="intro-spark"
              style={{ "--angle": `${-150 + i * 20}deg` } as React.CSSProperties}
            />
          ))}
        </div>

        {/* How it works: the shortcut, a query, the matches */}
        {scene === 2 && (
          <div className={`intro-demo step-${step}`}>
            <div className="intro-keys">
              {keys.map((key, i) => (
                <React.Fragment key={key}>
                  {i > 0 && <span>+</span>}
                  <kbd>{key}</kbd>
                </React.Fragment>
              ))}
            </div>
            <div className="intro-panel">
              <div className="intro-bar">
                <div className="spark" />
                <span>{demoQuery.slice(0, typed)}</span>
                <span className="intro-caret" />
              </div>
              {demoRows.map(({ name, file_type, icon }, i) => (
                <div
                  key={name}
                  className={`intro-row ${i === 0 ? "selected" : ""}`}
                  style={{ "--hue": `var(--hue-${file_type})`, "--row": i } as React.CSSProperties}
                >
                  {icon ? (
                    <img className="intro-row-icon" src={icon} alt="" draggable={false} />
                  ) : (
                    <span className="intro-row-icon plain" />
                  )}
                  <span className="intro-row-name">
                    <b>{name.slice(0, queryLength)}</b>
                    {name.slice(queryLength)}
                  </span>
                  {i === 0 && <kbd>Enter</kbd>}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* The words; re-keyed per scene so each one animates in */}
      <div className="intro-caption" key={scene}>
        {scene === 0 && (
          <>
            <p>{t("intro1a")}</p>
            <p className="later">{t("intro1b")}</p>
          </>
        )}
        {scene === 1 && (
          <>
            <p>{t("intro2")}</p>
            <h1 className="intro-name">Matchstick</h1>
          </>
        )}
        {scene === 2 && (
          <ol className="intro-steps">
            <li className={step === 0 ? "active" : ""}>
              {beforeHotkey}
              <kbd>{hotkey}</kbd>
              {afterHotkey}
            </li>
            <li className={step === 1 ? "active" : ""}>{t("introHow2")}</li>
            <li className={step === 2 ? "active" : ""}>{t("introHow3")}</li>
          </ol>
        )}
        {scene === 3 && (
          <>
            <p className="intro-strong">{t("intro4a")}</p>
            <p className="later">{t("intro4b")}</p>
            <button className="button primary intro-start" onClick={finish} autoFocus>
              {t("introStart")} <kbd>Enter</kbd>
            </button>
          </>
        )}
      </div>

      <div className="intro-footer">
        <div className="intro-dots">
          {[0, 1, 2, 3].map((i) => (
            <span key={i} className={i === scene ? "active" : ""} />
          ))}
        </div>
        {scene < LAST_SCENE && (
          <button className="footer-button" onClick={finish} tabIndex={-1}>
            {t("skip")} <kbd>Esc</kbd>
          </button>
        )}
      </div>
    </div>
  );
};

export default Welcome;
