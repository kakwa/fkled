import {
  useEffect,
  useRef,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { OFF_COLOR, paletteColor } from '../palette';
import { HEIGHT, WIDTH } from '../types';

const CELL_SIZE = 12;
const CANVAS_WIDTH = WIDTH * CELL_SIZE;
const CANVAS_HEIGHT = HEIGHT * CELL_SIZE;

type Cell = { row: number; col: number };

type Props = {
  pixels: number[];
  previousPixels: number[] | null;
  onionSkinning: boolean;
  onionAlpha: number;
  onPaint: (row: number, col: number) => void;
};

function lineCells(from: Cell, to: Cell): Cell[] {
  const cells: Cell[] = [];
  let x0 = from.col;
  let y0 = from.row;
  const x1 = to.col;
  const y1 = to.row;
  const deltaX = Math.abs(x1 - x0);
  const stepX = x0 < x1 ? 1 : -1;
  const deltaY = -Math.abs(y1 - y0);
  const stepY = y0 < y1 ? 1 : -1;
  let error = deltaX + deltaY;

  for (;;) {
    cells.push({ row: y0, col: x0 });
    if (x0 === x1 && y0 === y1) {
      break;
    }
    const doubled = 2 * error;
    if (doubled >= deltaY) {
      error += deltaY;
      x0 += stepX;
    }
    if (doubled <= deltaX) {
      error += deltaX;
      y0 += stepY;
    }
  }

  return cells;
}

export function PixelCanvas({
  pixels,
  previousPixels,
  onionSkinning,
  onionAlpha,
  onPaint,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const paintingRef = useRef(false);
  const lastCellRef = useRef<Cell | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!context) {
      return;
    }

    context.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    context.fillStyle = OFF_COLOR;
    context.fillRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);

    if (onionSkinning && previousPixels) {
      context.globalAlpha = onionAlpha;
      for (let row = 0; row < HEIGHT; row += 1) {
        for (let col = 0; col < WIDTH; col += 1) {
          const index = pixelsFor(row, col);
          if ((previousPixels[index] ?? 0) !== 0) {
            context.fillStyle = paletteColor(previousPixels[index]);
            context.fillRect(
              col * CELL_SIZE,
              row * CELL_SIZE,
              CELL_SIZE,
              CELL_SIZE,
            );
          }
        }
      }
      context.globalAlpha = 1;
    }

    for (let row = 0; row < HEIGHT; row += 1) {
      for (let col = 0; col < WIDTH; col += 1) {
        const value = pixels[pixelsFor(row, col)] ?? 0;
        if (value !== 0) {
          context.fillStyle = paletteColor(value);
          context.fillRect(
            col * CELL_SIZE,
            row * CELL_SIZE,
            CELL_SIZE,
            CELL_SIZE,
          );
        }
      }
    }

    context.strokeStyle = 'rgba(255, 255, 255, 0.07)';
    context.lineWidth = 1;
    for (let col = 0; col <= WIDTH; col += 1) {
      const x = col * CELL_SIZE + 0.5;
      context.beginPath();
      context.moveTo(x, 0);
      context.lineTo(x, CANVAS_HEIGHT);
      context.stroke();
    }
    for (let row = 0; row <= HEIGHT; row += 1) {
      const y = row * CELL_SIZE + 0.5;
      context.beginPath();
      context.moveTo(0, y);
      context.lineTo(CANVAS_WIDTH, y);
      context.stroke();
    }
  }, [pixels, previousPixels, onionSkinning, onionAlpha]);

  function cellAt(event: ReactPointerEvent<HTMLCanvasElement>): Cell | null {
    const canvas = canvasRef.current;
    if (!canvas) {
      return null;
    }
    const bounds = canvas.getBoundingClientRect();
    const col = Math.floor(((event.clientX - bounds.left) / bounds.width) * WIDTH);
    const row = Math.floor(((event.clientY - bounds.top) / bounds.height) * HEIGHT);
    if (row < 0 || row >= HEIGHT || col < 0 || col >= WIDTH) {
      return null;
    }
    return { row, col };
  }

  function paint(event: ReactPointerEvent<HTMLCanvasElement>) {
    const cell = cellAt(event);
    if (!cell) {
      return;
    }
    const last = lastCellRef.current;
    if (last) {
      for (const step of lineCells(last, cell)) {
        onPaint(step.row, step.col);
      }
    } else {
      onPaint(cell.row, cell.col);
    }
    lastCellRef.current = cell;
  }

  function handlePointerDown(event: ReactPointerEvent<HTMLCanvasElement>) {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    paintingRef.current = true;
    lastCellRef.current = null;
    paint(event);
  }

  function handlePointerMove(event: ReactPointerEvent<HTMLCanvasElement>) {
    if (paintingRef.current) {
      paint(event);
    }
  }

  function stopPainting(event: ReactPointerEvent<HTMLCanvasElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    paintingRef.current = false;
    lastCellRef.current = null;
  }

  return (
    <canvas
      ref={canvasRef}
      className="pixel-canvas"
      width={CANVAS_WIDTH}
      height={CANVAS_HEIGHT}
      aria-label="96 by 16 LED pixel editor"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={stopPainting}
      onPointerCancel={stopPainting}
      onLostPointerCapture={stopPainting}
    />
  );
}

function pixelsFor(row: number, col: number): number {
  return row * WIDTH + col;
}
