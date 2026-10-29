import { useCallback, useEffect, useState } from 'react';
import * as api from './api';
import './App.css';
import { MenuBar, type ServerStatus } from './components/MenuBar';
import { PixelCanvas } from './components/PixelCanvas';
import { TextPanel } from './components/TextPanel';
import { Timeline } from './components/Timeline';
import { ToolPanel } from './components/ToolPanel';
import { downloadBase64, downloadBlob, fileToBase64 } from './download';
import { getTextWidth, renderText } from './font';
import {
  HEIGHT,
  MAX_FXD_FRAMES,
  PIXEL_COUNT,
  WIDTH,
  createAnimation,
  createFrame,
  normaliseAnimation,
  type Animation,
  type Clipboard,
  type Frame,
  type Notice,
  type PcfFont,
  type Tool,
} from './types';

function applyBrush(
  pixels: number[],
  row: number,
  col: number,
  color: number,
  size: number,
): number[] {
  const next = pixels.slice();
  const start = -Math.floor((size - 1) / 2);
  const end = start + size - 1;
  for (let deltaRow = start; deltaRow <= end; deltaRow += 1) {
    for (let deltaCol = start; deltaCol <= end; deltaCol += 1) {
      const targetRow = row + deltaRow;
      const targetCol = col + deltaCol;
      if (
        targetRow >= 0 &&
        targetRow < HEIGHT &&
        targetCol >= 0 &&
        targetCol < WIDTH
      ) {
        next[targetRow * WIDTH + targetCol] = color & 0x0f;
      }
    }
  }
  return next;
}

function floodFill(
  pixels: number[],
  row: number,
  col: number,
  color: number,
): number[] {
  const start = row * WIDTH + col;
  const target = pixels[start] ?? 0;
  const replacement = color & 0x0f;
  if (target === replacement) {
    return pixels;
  }

  const next = pixels.slice();
  const stack: Array<[number, number]> = [[row, col]];
  while (stack.length > 0) {
    const [currentRow, currentCol] = stack.pop()!;
    if (
      currentRow < 0 ||
      currentRow >= HEIGHT ||
      currentCol < 0 ||
      currentCol >= WIDTH
    ) {
      continue;
    }
    const index = currentRow * WIDTH + currentCol;
    if (next[index] !== target) {
      continue;
    }
    next[index] = replacement;
    stack.push(
      [currentRow - 1, currentCol],
      [currentRow + 1, currentCol],
      [currentRow, currentCol - 1],
      [currentRow, currentCol + 1],
    );
  }
  return next;
}

function shiftedPixels(source: number[], shiftX: number, shiftY: number): number[] {
  const next = new Array<number>(PIXEL_COUNT).fill(0);
  for (let row = 0; row < HEIGHT; row += 1) {
    for (let col = 0; col < WIDTH; col += 1) {
      const sourceRow = row - shiftY;
      const sourceCol = col - shiftX;
      if (
        sourceRow >= 0 &&
        sourceRow < HEIGHT &&
        sourceCol >= 0 &&
        sourceCol < WIDTH
      ) {
        next[row * WIDTH + col] = source[sourceRow * WIDTH + sourceCol] ?? 0;
      }
    }
  }
  return next;
}

function safeFileName(name: string, extension: string): string {
  const stem = name
    .trim()
    .replace(/[^a-zA-Z0-9 _-]/g, '_')
    .slice(0, 80)
    .trim();
  return `${stem || 'animation'}.${extension}`;
}

export default function App() {
  const [animation, setAnimation] = useState<Animation>(() => createAnimation());
  const [currentFrame, setCurrentFrame] = useState(0);
  const [tool, setTool] = useState<Tool>('pencil');
  const [brushSize, setBrushSize] = useState(1);
  const [selectedColor, setSelectedColor] = useState(3);
  const [clipboard, setClipboard] = useState<Clipboard>(() => ({
    pixels: new Array<number>(PIXEL_COUNT).fill(0),
    hasData: false,
  }));
  const [shiftX, setShiftX] = useState(0);
  const [shiftY, setShiftY] = useState(0);
  const [onionSkinning, setOnionSkinning] = useState(false);
  const [onionAlpha, setOnionAlpha] = useState(0.3);
  const [playing, setPlaying] = useState(false);
  const [font, setFont] = useState<PcfFont | null>(null);
  const [fontName, setFontName] = useState<string | null>(null);
  const [text, setText] = useState('');
  const [textX, setTextX] = useState(0);
  const [textY, setTextY] = useState(0);
  const [textColor, setTextColor] = useState(3);
  const [scrollText, setScrollText] = useState('');
  const [scrollSpeed, setScrollSpeed] = useState(1);
  const [scrollGap, setScrollGap] = useState(2);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [serverStatus, setServerStatus] = useState<ServerStatus>('checking');

  const frame = animation.frames[currentFrame] ?? animation.frames[0];

  useEffect(() => {
    let cancelled = false;
    api
      .health()
      .then(() => {
        if (!cancelled) {
          setServerStatus('online');
        }
      })
      .catch(() => {
        if (!cancelled) {
          setServerStatus('offline');
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!notice || notice.kind === 'error') {
      return;
    }
    const timeout = window.setTimeout(() => setNotice(null), 4000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    if (!playing) {
      return;
    }
    const activeFrame = animation.frames[currentFrame];
    if (!activeFrame) {
      return;
    }
    const timeout = window.setTimeout(() => {
      setCurrentFrame(
        (index) => (index + 1) % Math.max(1, animation.frames.length),
      );
    }, Math.max(10, activeFrame.duration_ms));
    return () => window.clearTimeout(timeout);
  }, [playing, currentFrame, animation.frames]);

  const runTask = useCallback(async (task: () => Promise<void>) => {
    setBusy(true);
    try {
      await task();
    } catch (error) {
      setNotice({
        kind: 'error',
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setBusy(false);
    }
  }, []);

  const updateCurrentFrame = useCallback(
    (updater: (frame: Frame) => Frame) => {
      setAnimation((previous) => {
        const index = Math.min(
          currentFrame,
          Math.max(0, previous.frames.length - 1),
        );
        const activeFrame = previous.frames[index];
        if (!activeFrame) {
          return previous;
        }
        const nextFrame = updater(activeFrame);
        if (nextFrame === activeFrame) {
          return previous;
        }
        const frames = previous.frames.slice();
        frames[index] = nextFrame;
        return { ...previous, frames };
      });
    },
    [currentFrame],
  );

  const paintCell = useCallback(
    (row: number, col: number) => {
      setPlaying(false);
      setAnimation((previous) => {
        const index = Math.min(
          currentFrame,
          Math.max(0, previous.frames.length - 1),
        );
        const activeFrame = previous.frames[index];
        if (!activeFrame) {
          return previous;
        }
        const pixels =
          tool === 'fill'
            ? floodFill(activeFrame.pixels, row, col, selectedColor)
            : applyBrush(
                activeFrame.pixels,
                row,
                col,
                tool === 'eraser' ? 0 : selectedColor,
                brushSize,
              );
        if (pixels === activeFrame.pixels) {
          return previous;
        }
        const frames = previous.frames.slice();
        frames[index] = { ...activeFrame, pixels };
        return { ...previous, frames };
      });
    },
    [brushSize, currentFrame, selectedColor, tool],
  );

  function handleNew() {
    if (
      !window.confirm('Start a new project? Unsaved changes will be lost.')
    ) {
      return;
    }
    setAnimation(createAnimation());
    setCurrentFrame(0);
    setPlaying(false);
    setClipboard((previous) => ({ ...previous, hasData: false }));
    setNotice({ kind: 'info', text: 'New project created.' });
  }

  function handleAddFrame() {
    setAnimation((previous) => {
      const index = Math.min(
        currentFrame,
        Math.max(0, previous.frames.length - 1),
      );
      const frames = previous.frames.slice();
      frames.splice(index + 1, 0, createFrame());
      return { ...previous, frames };
    });
    setCurrentFrame((index) =>
      Math.min(index + 1, animation.frames.length),
    );
    setPlaying(false);
  }

  function handleDuplicateFrame() {
    const activeFrame = animation.frames[currentFrame];
    if (!activeFrame) {
      return;
    }
    const clone = createFrame(activeFrame.pixels, activeFrame.duration_ms);
    setAnimation((previous) => {
      const index = Math.min(
        currentFrame,
        Math.max(0, previous.frames.length - 1),
      );
      const frames = previous.frames.slice();
      frames.splice(index + 1, 0, clone);
      return { ...previous, frames };
    });
    setCurrentFrame((index) =>
      Math.min(index + 1, animation.frames.length),
    );
    setPlaying(false);
  }

  function handleDeleteFrame() {
    if (animation.frames.length <= 1) {
      return;
    }
    setAnimation((previous) => {
      const index = Math.min(
        currentFrame,
        Math.max(0, previous.frames.length - 1),
      );
      const frames = previous.frames.slice();
      frames.splice(index, 1);
      return { ...previous, frames };
    });
    setCurrentFrame((index) =>
      Math.max(0, Math.min(index, animation.frames.length - 2)),
    );
    setPlaying(false);
  }

  function handleCopyFrame() {
    const activeFrame = animation.frames[currentFrame];
    if (!activeFrame) {
      return;
    }
    setClipboard({ pixels: activeFrame.pixels.slice(), hasData: true });
    setNotice({ kind: 'info', text: 'Frame copied to the clipboard.' });
  }

  function handlePasteFrame() {
    if (!clipboard.hasData) {
      return;
    }
    updateCurrentFrame((activeFrame) => ({
      ...activeFrame,
      pixels: clipboard.pixels.slice(),
    }));
    setPlaying(false);
  }

  function handlePasteShifted() {
    if (!clipboard.hasData) {
      return;
    }
    const pixels = shiftedPixels(clipboard.pixels, shiftX, shiftY);
    updateCurrentFrame((activeFrame) => ({ ...activeFrame, pixels }));
    setPlaying(false);
  }

  function handleOpenProject(file: File) {
    void runTask(async () => {
      const parsed = normaliseAnimation(JSON.parse(await file.text()));
      setAnimation(parsed);
      setCurrentFrame(0);
      setPlaying(false);
      setNotice({
        kind: 'info',
        text: `Opened ${file.name} (${parsed.frames.length} frames).`,
      });
    });
  }

  function handleSaveProject() {
    const json = JSON.stringify(animation, null, 2);
    downloadBlob(
      safeFileName(animation.name, 'ledfx.json'),
      'application/json',
      new TextEncoder().encode(json),
    );
    setNotice({ kind: 'info', text: 'Project JSON downloaded.' });
  }

  function handleImportFxd(file: File) {
    void runTask(async () => {
      const result = await api.importUsbFxd(await fileToBase64(file));
      const imported = normaliseAnimation(result.animation);
      setAnimation(imported);
      setCurrentFrame(0);
      setPlaying(false);
      setNotice({
        kind: 'info',
        text: `Imported ${file.name} (${imported.frames.length} frames).`,
      });
    });
  }

  function handleExportFxd() {
    if (animation.frames.length > MAX_FXD_FRAMES) {
      setNotice({
        kind: 'error',
        text: `USB.FXD supports at most ${MAX_FXD_FRAMES} frames; this project has ${animation.frames.length}.`,
      });
      return;
    }
    void runTask(async () => {
      const payload = await api.exportUsbFxd(animation);
      downloadBase64(payload.filename, payload.mime_type, payload.data);
      setNotice({
        kind: 'info',
        text: `${payload.filename} generated by the server.`,
      });
    });
  }

  function handleImportGif(file: File) {
    void runTask(async () => {
      const result = await api.importGif(await fileToBase64(file));
      const imported = normaliseAnimation(result.animation);
      setAnimation(imported);
      setCurrentFrame(0);
      setPlaying(false);
      setNotice({
        kind: 'info',
        text: `Imported ${file.name} (${imported.frames.length} frames).`,
      });
    });
  }

  function handleExportGif() {
    void runTask(async () => {
      const payload = await api.exportGif(animation);
      downloadBase64(payload.filename, payload.mime_type, payload.data);
      setNotice({
        kind: 'info',
        text: `${payload.filename} generated by the server.`,
      });
    });
  }

  function handleLoadFont(file: File) {
    void runTask(async () => {
      const result = await api.parseFont(await fileToBase64(file));
      setFont(result.font);
      setFontName(file.name);
      setNotice({
        kind: 'info',
        text: `Loaded ${file.name} (${Object.keys(result.font.glyphs).length} glyphs).`,
      });
    });
  }

  function handleRenderText() {
    if (!font) {
      setNotice({ kind: 'error', text: 'Load a PCF font first.' });
      return;
    }
    updateCurrentFrame((activeFrame) => {
      const pixels = activeFrame.pixels.slice();
      renderText(font, text, textX, textY, textColor, pixels);
      return { ...activeFrame, pixels };
    });
    setPlaying(false);
  }

  function handleGenerateScroll() {
    if (!font) {
      setNotice({ kind: 'error', text: 'Load a PCF font first.' });
      return;
    }
    if (!scrollText) {
      setNotice({ kind: 'error', text: 'Enter some marquee text first.' });
      return;
    }

    const fullText = `${scrollText}${' '.repeat(Math.max(0, scrollGap))}${scrollText}`;
    const textWidth = getTextWidth(font, fullText);
    const step = Math.max(1, scrollSpeed);
    const frames: Frame[] = [];
    for (let offset = -textWidth; offset < WIDTH; offset += step) {
      const pixels = new Array<number>(PIXEL_COUNT).fill(0);
      renderText(font, fullText, offset, textY, textColor, pixels);
      frames.push({ pixels, duration_ms: 50 });
    }

    if (frames.length === 0) {
      return;
    }

    setAnimation((previous) => ({ ...previous, frames }));
    setCurrentFrame(0);
    setPlaying(false);
    setNotice({
      kind: 'info',
      text: `Generated ${frames.length} scrolling frames.`,
    });
  }

  const previousPixels =
    onionSkinning && currentFrame > 0
      ? (animation.frames[currentFrame - 1]?.pixels ?? null)
      : null;

  return (
    <div className="app">
      <MenuBar
        name={animation.name}
        onNameChange={(name) =>
          setAnimation((previous) => ({ ...previous, name }))
        }
        serverStatus={serverStatus}
        busy={busy}
        notice={notice}
        onDismissNotice={() => setNotice(null)}
        onNew={handleNew}
        onOpenProject={handleOpenProject}
        onSaveProject={handleSaveProject}
        onImportFxd={handleImportFxd}
        onExportFxd={handleExportFxd}
        onImportGif={handleImportGif}
        onExportGif={handleExportGif}
      />

      <div className="workspace">
        <aside className="sidebar left">
          <ToolPanel
            tool={tool}
            onToolChange={setTool}
            brushSize={brushSize}
            onBrushSizeChange={(size) =>
              setBrushSize(Math.min(3, Math.max(1, size)))
            }
            selectedColor={selectedColor}
            onSelectedColorChange={setSelectedColor}
            clipboard={clipboard}
            onCopyFrame={handleCopyFrame}
            onPasteFrame={handlePasteFrame}
            onPasteShifted={handlePasteShifted}
            shiftX={shiftX}
            shiftY={shiftY}
            onShiftXChange={setShiftX}
            onShiftYChange={setShiftY}
            onionSkinning={onionSkinning}
            onOnionSkinningChange={setOnionSkinning}
            onionAlpha={onionAlpha}
            onOnionAlphaChange={setOnionAlpha}
          />
        </aside>

        <main className="stage">
          <div className="stage-heading">
            <div>
              <span className="eyebrow">Canvas</span>
              <h1>96 × 16 LED panel</h1>
            </div>
            <div className="stage-meta">
              <span className="chip">{animation.frames.length} frames</span>
              <span className="chip">
                {frame?.duration_ms ?? 100} ms / frame
              </span>
            </div>
          </div>

          <div className="canvas-card">
            <PixelCanvas
              pixels={frame?.pixels ?? []}
              previousPixels={previousPixels}
              onionSkinning={onionSkinning}
              onionAlpha={onionAlpha}
              onPaint={paintCell}
            />
          </div>

          <div className="canvas-footer">
            <p>
              Left-drag to draw. The canvas is transmitted as part of the
              project JSON when the server generates a USB.FXD or GIF file.
            </p>
            <span className="pixel-count">{WIDTH * HEIGHT} pixels</span>
          </div>
        </main>

        <aside className="sidebar right">
          <TextPanel
            font={font}
            fontName={fontName}
            onLoadFont={handleLoadFont}
            text={text}
            onTextChange={setText}
            textX={textX}
            textY={textY}
            textColor={textColor}
            onTextXChange={setTextX}
            onTextYChange={setTextY}
            onTextColorChange={setTextColor}
            onRenderText={handleRenderText}
            scrollText={scrollText}
            scrollSpeed={scrollSpeed}
            scrollGap={scrollGap}
            onScrollTextChange={setScrollText}
            onScrollSpeedChange={setScrollSpeed}
            onScrollGapChange={setScrollGap}
            onGenerateScroll={handleGenerateScroll}
          />
        </aside>
      </div>

      <Timeline
        frames={animation.frames}
        currentFrame={currentFrame}
        playing={playing}
        onSelect={(index) => {
          setCurrentFrame(index);
          setPlaying(false);
        }}
        onFirst={() => {
          setCurrentFrame(0);
          setPlaying(false);
        }}
        onPrevious={() => {
          setPlaying(false);
          setCurrentFrame((index) =>
            index === 0 ? animation.frames.length - 1 : index - 1,
          );
        }}
        onNext={() => {
          setPlaying(false);
          setCurrentFrame((index) => (index + 1) % animation.frames.length);
        }}
        onLast={() => {
          setCurrentFrame(animation.frames.length - 1);
          setPlaying(false);
        }}
        onTogglePlay={() => setPlaying((value) => !value)}
        onAdd={handleAddFrame}
        onDuplicate={handleDuplicateFrame}
        onDelete={handleDeleteFrame}
        onDurationChange={(durationMs) =>
          updateCurrentFrame((activeFrame) => ({
            ...activeFrame,
            duration_ms: durationMs,
          }))
        }
      />
    </div>
  );
}
