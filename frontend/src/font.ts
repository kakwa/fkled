import { HEIGHT, WIDTH, type PcfFont } from './types';

export function getTextWidth(font: PcfFont, text: string): number {
  let width = 0;
  for (const character of text) {
    const glyph = font.glyphs[character];
    width += glyph ? glyph.character_width : 4;
  }
  return width;
}

/**
 * Draw PCF glyphs into a flat 96x16 pixel buffer. This mirrors the server-side
 * `pcf::render_text` implementation, while the PCF parsing itself stays on the
 * Rust server.
 */
export function renderText(
  font: PcfFont,
  text: string,
  startX: number,
  startY: number,
  color: number,
  pixels: number[],
): void {
  const firstGlyph = Object.values(font.glyphs)[0];
  const referenceAscent = firstGlyph?.ascent ?? 0;
  const paletteIndex = color & 0x0f;
  let x = startX;

  for (const character of text) {
    const glyph = font.glyphs[character];
    if (glyph) {
      const glyphY = startY + referenceAscent - glyph.ascent;
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
    } else if (font.default_char && font.glyphs[font.default_char]) {
      x += font.glyphs[font.default_char].character_width;
    } else {
      x += 4;
    }
  }
}
