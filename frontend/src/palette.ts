import { normalizeColor } from './types';

export type PaletteEntry = {
  /** Palette index stored in the project JSON and USB.FXD data. */
  index: number;
  color: string;
  name: string;
};

/**
 * The eight colours the LED panel can display. Index bits are BGR: bit 2 =
 * blue, bit 1 = green, bit 0 = red. Values 8–15 are not displayable and are
 * normalized to black.
 */
export const PALETTE: PaletteEntry[] = [
  { index: 0, color: '#000000', name: 'Black' },
  { index: 1, color: '#ff0000', name: 'Red' },
  { index: 2, color: '#00ff00', name: 'Green' },
  { index: 3, color: '#ffff00', name: 'Yellow' },
  { index: 4, color: '#0000ff', name: 'Blue' },
  { index: 5, color: '#ff00ff', name: 'Magenta' },
  { index: 6, color: '#00ffff', name: 'Cyan' },
  { index: 7, color: '#ffffff', name: 'White' },
];

/** Colour table used to render pixels. Mirrors the Rust `palette` module. */
const RENDER_PALETTE = PALETTE.map((entry) => entry.color);

export const OFF_COLOR = '#15171c';

export function paletteColor(index: number): string {
  return RENDER_PALETTE[normalizeColor(index)] ?? RENDER_PALETTE[0];
}

export function paletteEntry(index: number): PaletteEntry {
  const normalized = normalizeColor(index);
  return PALETTE.find((entry) => entry.index === normalized) ?? PALETTE[0];
}
