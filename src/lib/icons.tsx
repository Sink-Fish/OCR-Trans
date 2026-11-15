/** Original 16×16 line icons (1.5 px strokes, currentColor). */
import type { ReactElement } from "react";

export type ActionId = "translate" | "ocr" | "pin" | "doodle" | "copy" | "save";
export type IconId =
  | ActionId
  | "cancel"
  | "move"
  | "select"
  | "pen"
  | "highlight"
  | "rect"
  | "ellipse"
  | "arrow"
  | "text"
  | "marker"
  | "mosaic"
  | "undo"
  | "redo"
  | "delete"
  | "opacity"
  | "close"
  | "image"
  | "gear"
  | "box"
  | "info"
  | "clock"
  | "check"
  | "keyboard"
  | "language"
  | "eye"
  | "shield"
  | "download"
  | "folder"
  | "play";

const P: Record<IconId, ReactElement> = {
  // corner brackets with two lines of text → "recognise"
  ocr: (
    <>
      <path d="M2 5V3a1 1 0 0 1 1-1h2M11 2h2a1 1 0 0 1 1 1v2M14 11v2a1 1 0 0 1-1 1h-2M5 14H3a1 1 0 0 1-1-1v-2" />
      <path d="M5 6.5h6M5 9.5h4" />
    </>
  ),
  // Latin "A" with a small arrow into a CJK-like block → "translate"
  translate: (
    <>
      <path d="M2 12l2.6-7h.8L8 12M3 9.8h4" />
      <path d="M9.5 4h4.5M11.75 3v1M10.2 4c.6 2.4 2 4.3 3.8 5.2M13.3 4c-.5 2.2-1.7 4.1-3.6 5.4" />
    </>
  ),
  pin: (
    <>
      <path d="M9.5 2.5l4 4-1.6.6-2.4 2.4.3 2.6-1 1-2.6-2.6L3 13.7M8.2 11l-3.2-3.2 1-1 2.6.3 2.4-2.4.5-1.7" />
    </>
  ),
  doodle: <path d="M3 13l.7-2.8L10.9 3a1.4 1.4 0 0 1 2 2L5.8 12.3 3 13zM9.8 4.1l2 2" />,
  copy: (
    <>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
      <path d="M10.5 5.5V3.8A1.3 1.3 0 0 0 9.2 2.5H3.8a1.3 1.3 0 0 0-1.3 1.3v5.4a1.3 1.3 0 0 0 1.3 1.3h1.7" />
    </>
  ),
  save: <path d="M8 2.5v7.5M4.8 7l3.2 3.2L11.2 7M3 12.5v.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-.5" />,
  cancel: <path d="M4 4l8 8M12 4l-8 8" />,
  close: <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />,
  move: <path d="M8 2v12M2 8h12M8 2L6.5 3.5M8 2l1.5 1.5M8 14l-1.5-1.5M8 14l1.5-1.5M2 8l1.5-1.5M2 8l1.5 1.5M14 8l-1.5-1.5M14 8l-1.5 1.5" />,
  select: <path d="M3.5 2.5l8.5 5-3.8 1 2.3 4-1.4.8-2.3-4-2.6 2.8z" />,
  pen: <path d="M3 13l.7-2.8L10.9 3a1.4 1.4 0 0 1 2 2L5.8 12.3 3 13z" />,
  highlight: (
    <>
      <path d="M5.5 10.5l5.8-7.3 2 1.6-5.8 7.3-2.6.6.6-2.2z" />
      <path d="M2.5 14h11" strokeOpacity=".5" />
    </>
  ),
  rect: <rect x="2.5" y="3.5" width="11" height="9" rx="1" />,
  ellipse: <ellipse cx="8" cy="8" rx="5.5" ry="4.5" />,
  arrow: <path d="M3 13L13 3M7 3h6v6" />,
  text: <path d="M3.5 4V3h9v1M8 3v10M6.2 13h3.6" />,
  marker: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M7 6.2L8.3 5.5V10.5M7 10.5h2.6" />
    </>
  ),
  mosaic: (
    <>
      <rect x="2.5" y="2.5" width="11" height="11" rx="1" />
      <path d="M2.5 6.2h11M2.5 9.8h11M6.2 2.5v11M9.8 2.5v11" strokeOpacity=".6" />
    </>
  ),
  undo: <path d="M5.5 4L3 6.5 5.5 9M3 6.5h6.5a3.5 3.5 0 0 1 0 7H7" />,
  redo: <path d="M10.5 4L13 6.5 10.5 9M13 6.5H6.5a3.5 3.5 0 0 0 0 7H9" />,
  delete: <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" />,
  opacity: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 2.5a5.5 5.5 0 0 1 0 11z" fill="currentColor" stroke="none" />
    </>
  ),
  image: (
    <>
      <rect x="2.5" y="3" width="11" height="10" rx="1.5" />
      <path d="M2.5 11l3.2-3.2 2.6 2.6 1.7-1.7 3.5 3.3" />
      <circle cx="10.5" cy="6" r="1" />
    </>
  ),
  gear: (
    <>
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1.8v1.6M8 12.6v1.6M14.2 8h-1.6M3.4 8H1.8M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1M12.4 12.4l-1.1-1.1M4.7 4.7L3.6 3.6" />
    </>
  ),
  box: (
    <>
      <path d="M2.5 5L8 2.5 13.5 5v6L8 13.5 2.5 11z" />
      <path d="M2.5 5L8 7.5 13.5 5M8 7.5v6" />
    </>
  ),
  info: (
    <>
      <circle cx="8" cy="8" r="5.8" />
      <path d="M8 7.3v3.6M8 5.1v.1" />
    </>
  ),
  clock: (
    <>
      <circle cx="8" cy="8" r="5.8" />
      <path d="M8 4.8V8l2.2 1.4" />
    </>
  ),
  check: <path d="M3.5 8.3l2.8 2.7 6.2-6.3" />,
  keyboard: (
    <>
      <rect x="1.8" y="4" width="12.4" height="8" rx="2" />
      <path d="M4.5 6.6h.1M7 6.6h.1M9.5 6.6h.1M12 6.6h-.1M5 9.4h6" />
    </>
  ),
  language: (
    <>
      <circle cx="8" cy="8" r="5.8" />
      <path d="M2.4 8h11.2M8 2.2c1.7 1.6 2.5 3.6 2.5 5.8S9.7 12.2 8 13.8M8 2.2C6.3 3.8 5.5 5.8 5.5 8s.8 4.2 2.5 5.8" />
    </>
  ),
  eye: (
    <>
      <path d="M1.8 8s2.3-4.2 6.2-4.2S14.2 8 14.2 8 11.9 12.2 8 12.2 1.8 8 1.8 8z" />
      <circle cx="8" cy="8" r="1.8" />
    </>
  ),
  shield: <path d="M8 2l5 1.8v3.7c0 3-2.1 5.4-5 6.5-2.9-1.1-5-3.5-5-6.5V3.8z" />,
  download: <path d="M8 2.5v7.5M4.8 7l3.2 3.2L11.2 7M3 12.5v.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-.5" />,
  folder: <path d="M2 4.5a1 1 0 0 1 1-1h3l1.4 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" />,
  play: <path d="M5 3.5v9l7-4.5z" />,
};

/** Small line illustration for empty states: a selection frame around soft text lines. */
export function EmptyArt({ size = 96 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 96 96" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="10" y="18" width="76" height="60" rx="14" fill="currentColor" fillOpacity=".08" stroke="none" />
      <path d="M22 34v-6a4 4 0 0 1 4-4h6M64 24h6a4 4 0 0 1 4 4v6M74 62v6a4 4 0 0 1-4 4h-6M32 72h-6a4 4 0 0 1-4-4v-6" />
      <path d="M34 42h28M34 50h20M34 58h24" strokeOpacity=".55" />
      <circle cx="74" cy="20" r="3" fill="currentColor" stroke="none" fillOpacity=".5" />
      <circle cx="16" cy="80" r="2" fill="currentColor" stroke="none" fillOpacity=".35" />
    </svg>
  );
}

export function Icon({ id, size = 16 }: { id: IconId; size?: number }) {
  return (
    <svg className="icon" width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {P[id]}
    </svg>
  );
}

export function ActionIcon({ id }: { id: ActionId | "cancel" }) {
  return <Icon id={id} />;
}
