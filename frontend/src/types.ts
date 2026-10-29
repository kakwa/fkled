export const WIDTH = 96;
export const HEIGHT = 16;
export const PIXEL_COUNT = WIDTH * HEIGHT;
export const MAX_FXD_FRAMES = 32;
export const COLOR_COUNT = 8;

export type Frame = {
  /** Row-major palette indices, `WIDTH * HEIGHT` entries. */
  pixels: number[];
  duration_ms: number;
};

export type Animation = {
  frames: Frame[];
  name: string;
};

export type Tool = 'pencil' | 'eraser' | 'fill';

export type Clipboard = {
  pixels: number[];
  hasData: boolean;
};

export type Glyph = {
  width: number;
  height: number;
  bitmap: boolean[][];
  left_bearing: number;
  right_bearing: number;
  ascent: number;
  descent: number;
  character_width: number;
};

export type Font = {
  glyphs: Record<string, Glyph>;
  default_char: string | null;
  max_width: number;
  max_height: number;
  /** Distance from the top of the line to the baseline. */
  ascent: number;
  /** Distance from the baseline to the bottom of the line. */
  descent: number;
};

export type FilePayload = {
  filename: string;
  mime_type: string;
  data: string;
};

export type AnimationPayload = {
  animation: Animation;
};

export type FontPayload = {
  font: Font;
};

export type HealthPayload = {
  ok: boolean;
  service: string;
  version: string;
};

export type Notice = {
  kind: 'info' | 'error';
  text: string;
};

/**
 * Clamp a stored palette index to the eight colors the panel supports.
 * Unsupported values become black (index 0), the editor's background color.
 */
export function normalizeColor(index: number): number {
  return Number.isInteger(index) && index >= 0 && index < COLOR_COUNT ? index : 0;
}

export function pixelIndex(row: number, col: number): number {
  return row * WIDTH + col;
}

export function createFrame(pixels?: number[], durationMs = 100): Frame {
  const data = new Array<number>(PIXEL_COUNT).fill(0);
  if (pixels) {
    for (let index = 0; index < PIXEL_COUNT; index += 1) {
      data[index] = normalizeColor(pixels[index] ?? 0);
    }
  }
  return { pixels: data, duration_ms: durationMs };
}

export function createAnimation(): Animation {
  return { frames: [createFrame()], name: 'Untitled' };
}

function sanitiseFrame(value: unknown): Frame {
  if (!value || typeof value !== 'object') {
    return createFrame();
  }
  const frame = value as Partial<Frame>;
  const duration =
    typeof frame.duration_ms === 'number' && Number.isFinite(frame.duration_ms)
      ? Math.min(1000, Math.max(10, Math.round(frame.duration_ms)))
      : 100;
  return createFrame(Array.isArray(frame.pixels) ? frame.pixels : undefined, duration);
}

export function normaliseAnimation(value: unknown): Animation {
  if (!value || typeof value !== 'object') {
    throw new Error('The selected file does not contain an animation.');
  }
  const animation = value as Partial<Animation>;
  if (!Array.isArray(animation.frames)) {
    throw new Error('The selected file is missing an animation frame list.');
  }
  const frames = animation.frames.map(sanitiseFrame);
  return {
    name:
      typeof animation.name === 'string' && animation.name.trim()
        ? animation.name.trim()
        : 'Untitled',
    frames: frames.length > 0 ? frames : [createFrame()],
  };
}
