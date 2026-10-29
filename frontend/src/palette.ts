export type PaletteEntry = {
  /** Palette index stored in the project JSON and USB.FXD data. */
  index: number;
  color: string;
  name: string;
};

/**
 * The eight colours offered by the editor. The indices are kept equal to the
 * full 16-colour format so that imported USB.FXD/GIF animations keep their
 * original colours.
 */
export const PALETTE: PaletteEntry[] = [
  { index: 0, color: '#000000', name: 'Black' },
  { index: 3, color: '#ffffff', name: 'White' },
  { index: 5, color: '#ff5555', name: 'Red' },
  { index: 7, color: '#ffff55', name: 'Yellow' },
  { index: 13, color: '#5555ff', name: 'Blue' },
  { index: 11, color: '#55ffff', name: 'Cyan' },
  { index: 15, color: '#ff55ff', name: 'Magenta' },
  { index: 9, color: '#55ff55', name: 'Green' },
];

/** Full 16-entry table used to render imported projects. */
const RENDER_PALETTE = [
  '#000000',
  '#555555',
  '#aaaaaa',
  '#ffffff',
  '#aa0000',
  '#ff5555',
  '#aa5500',
  '#ffff55',
  '#00aa00',
  '#55ff55',
  '#00aaaa',
  '#55ffff',
  '#0000aa',
  '#5555ff',
  '#aa00aa',
  '#ff55ff',
] as const;

export const OFF_COLOR = '#15171c';

export function paletteColor(index: number): string {
  return RENDER_PALETTE[index & 0x0f] ?? RENDER_PALETTE[0];
}

export function paletteEntry(index: number): PaletteEntry {
  return PALETTE.find((entry) => entry.index === index) ?? PALETTE[0];
}
