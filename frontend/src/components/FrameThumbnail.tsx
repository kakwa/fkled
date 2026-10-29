import { useEffect, useRef } from 'react';
import { OFF_COLOR, paletteColor } from '../palette';
import { HEIGHT, WIDTH } from '../types';

type Props = {
  pixels: number[];
};

export function FrameThumbnail({ pixels }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!context) {
      return;
    }
    context.fillStyle = OFF_COLOR;
    context.fillRect(0, 0, WIDTH, HEIGHT);
    for (let row = 0; row < HEIGHT; row += 1) {
      for (let col = 0; col < WIDTH; col += 1) {
        const value = pixels[row * WIDTH + col] ?? 0;
        if (value !== 0) {
          context.fillStyle = paletteColor(value);
          context.fillRect(col, row, 1, 1);
        }
      }
    }
  }, [pixels]);

  return (
    <canvas
      ref={canvasRef}
      className="frame-thumb"
      width={WIDTH}
      height={HEIGHT}
      aria-hidden="true"
    />
  );
}
