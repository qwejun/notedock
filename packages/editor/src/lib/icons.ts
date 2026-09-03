/**
 * Hand-rolled 24×24 stroke icons.
 *
 * A whole icon package would be the third-largest dependency in the project for
 * the dozen glyphs the UI actually needs, and the floating window ships every
 * byte to a webview. Marks (bold, italic, heading levels) use letters instead —
 * they read better than any pictogram at this size.
 */
export const ICON_PATHS = {
  bulletList: "M9 6h12M9 12h12M9 18h12",
  orderedList:
    "M10 6h11M10 12h11M10 18h11M4 6h1v4M4 10h2M6 18H4c0-1 2-2 2-3s-1-1.5-2-1",
  quote: "M6 16h3l2-4V7H5v6h3zM16 16h3l2-4V7h-6v6h3z",
  link: "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1",
  clear: "M13 4l7 7-8 8H7l-3-3zM9 20h11",
  textColor: "M5 19h14M8 15 12 5l4 10",
  highlight: "M9 11l-4 4v3h3l4-4M13 5l6 6-7 7-6-6z",
  image: "M3 5h18v14H3zM3 16l5-5 4 4 3-3 5 5",
  web: "M4 5h16v12H4zM8 21h8M12 17v4M8 9h8M8 13h5",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14M20 20l-4.5-4.5",
  check: "M4 12l5 5L20 6",
  close: "M6 6l12 12M18 6L6 18",
  minimize: "M5 12h14",
  plus: "M12 5v14M5 12h14",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
  pin: "M8 4h8l1 6 3 3v2h-7v5l-1 1-1-1v-5H4v-2l3-3z",
  contrast: "M12 3a9 9 0 1 0 0 18zM12 3a9 9 0 0 1 0 18",
  /**
   * Three sliders with knobs, drawn as gapped lines plus the rings in
   * {@link ICON_CIRCLES}. A cog would be the more conventional glyph, but a cog
   * is a forty-segment bezier that is easy to get subtly wrong; this is correct
   * by construction and, at 14px, more legible.
   */
  settings:
    "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.38a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
} as const;

export type IconName = keyof typeof ICON_PATHS;

/** A circle some glyph needs on top of its path. */
export interface IconCircle {
  cx: number;
  cy: number;
  r: number;
  /** Filled discs for list bullets; outlined rings for slider knobs. */
  filled?: boolean;
}

export const ICON_CIRCLES: Partial<Record<IconName, readonly IconCircle[]>> = {
  bulletList: [
    { cx: 4, cy: 6, r: 1.1, filled: true },
    { cx: 4, cy: 12, r: 1.1, filled: true },
    { cx: 4, cy: 18, r: 1.1, filled: true },
  ],
  settings: [{ cx: 12, cy: 12, r: 2.5 }],
};
