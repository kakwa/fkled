import { HEIGHT, WIDTH, normalizeColor, type Font, type Glyph } from './types';

function resolveGlyph(font: Font, character: string): Glyph | null {
  const glyph = font.glyphs[character];
  if (glyph) {
    return glyph;
  }
  if (font.default_char) {
    return font.glyphs[font.default_char] ?? null;
  }
  return null;
}

export function getTextWidth(font: Font, text: string): number {
  let width = 0;
  for (const character of text) {
    const glyph = resolveGlyph(font, character);
    width += glyph ? glyph.character_width : 4;
  }
  return width;
}

/**
 * Draw BDF glyphs into a flat 96x16 pixel buffer. This mirrors the server-side
 * `bdf::render_text` implementation, while the font parsing itself stays on
 * the Rust server.
 */
export function renderText(
  font: Font,
  text: string,
  startX: number,
  startY: number,
  color: number,
  pixels: number[],
): void {
  const firstGlyph = Object.values(font.glyphs)[0];
  const ascent = font.ascent ?? firstGlyph?.ascent ?? 0;
  const baseline = startY + ascent;
  const paletteIndex = normalizeColor(color);
  let x = startX;

  for (const character of text) {
    const glyph = resolveGlyph(font, character);
    if (!glyph) {
      x += 4;
      continue;
    }

    const glyphY = baseline - glyph.ascent;
    for (let row = 0; row < glyph.height; row += 1) {
      const bitmapRow = glyph.bitmap[row];
      if (!bitmapRow) {
        continue;
      }
      for (let col = 0; col < glyph.width; col += 1) {
        if (!bitmapRow[col]) {
          continue;
        }
        const pixelY = glyphY + row;
        const pixelX = x + glyph.left_bearing + col;
        if (
          pixelY >= 0 &&
          pixelY < HEIGHT &&
          pixelX >= 0 &&
          pixelX < WIDTH
        ) {
          pixels[pixelY * WIDTH + pixelX] = paletteIndex;
        }
      }
    }
    x += glyph.character_width;
  }
}
