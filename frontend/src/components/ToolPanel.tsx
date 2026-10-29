import { PALETTE, paletteEntry } from '../palette';
import type { Clipboard, Tool } from '../types';

type Props = {
  tool: Tool;
  onToolChange: (tool: Tool) => void;
  brushSize: number;
  onBrushSizeChange: (size: number) => void;
  selectedColor: number;
  onSelectedColorChange: (index: number) => void;
  clipboard: Clipboard;
  onCopyFrame: () => void;
  onPasteFrame: () => void;
  onPasteShifted: () => void;
  shiftX: number;
  shiftY: number;
  onShiftXChange: (value: number) => void;
  onShiftYChange: (value: number) => void;
  onionSkinning: boolean;
  onOnionSkinningChange: (enabled: boolean) => void;
  onionAlpha: number;
  onOnionAlphaChange: (alpha: number) => void;
};

const TOOLS: Array<{ id: Tool; label: string; hint: string }> = [
  { id: 'pencil', label: 'Pencil', hint: 'Paint with the selected colour' },
  { id: 'eraser', label: 'Eraser', hint: 'Clear pixels to colour 0' },
  { id: 'fill', label: 'Fill', hint: 'Flood fill a connected region' },
];

export function ToolPanel({
  tool,
  onToolChange,
  brushSize,
  onBrushSizeChange,
  selectedColor,
  onSelectedColorChange,
  clipboard,
  onCopyFrame,
  onPasteFrame,
  onPasteShifted,
  shiftX,
  shiftY,
  onShiftXChange,
  onShiftYChange,
  onionSkinning,
  onOnionSkinningChange,
  onionAlpha,
  onOnionAlphaChange,
}: Props) {
  return (
    <div className="panel-stack">
      <section className="panel-section">
        <h2>Tools</h2>
        <div className="tool-grid">
          {TOOLS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`btn tool-button ${tool === item.id ? 'active' : ''}`}
              title={item.hint}
              onClick={() => onToolChange(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <label className="field">
          <span>
            Brush size <strong>{brushSize}</strong>
          </span>
          <input
            type="range"
            min={1}
            max={3}
            step={1}
            value={brushSize}
            onChange={(event) => onBrushSizeChange(Number(event.target.value))}
          />
        </label>
      </section>

      <section className="panel-section">
        <h2>Palette</h2>
        <div className="palette-grid">
          {PALETTE.map((entry) => (
            <button
              key={entry.index}
              type="button"
              className={`swatch ${selectedColor === entry.index ? 'active' : ''}`}
              style={{ backgroundColor: entry.color }}
              title={`${entry.name} — ${entry.color}`}
              aria-label={`Select ${entry.name.toLowerCase()}`}
              aria-pressed={selectedColor === entry.index}
              onClick={() => onSelectedColorChange(entry.index)}
            />
          ))}
        </div>
        <p className="hint">
          Selected: <strong>{paletteEntry(selectedColor).name}</strong>
          <span
            className="inline-swatch"
            style={{ backgroundColor: paletteEntry(selectedColor).color }}
          />
        </p>
      </section>

      <section className="panel-section">
        <h2>Copy / paste</h2>
        <div className="button-row">
          <button type="button" className="btn" onClick={onCopyFrame}>
            Copy frame
          </button>
          <button
            type="button"
            className="btn"
            onClick={onPasteFrame}
            disabled={!clipboard.hasData}
          >
            Paste
          </button>
        </div>
        <div className="field-grid">
          <label className="field compact">
            <span>Shift X</span>
            <input
              type="number"
              value={shiftX}
              step={1}
              onChange={(event) =>
                onShiftXChange(Number.parseInt(event.target.value, 10) || 0)
              }
            />
          </label>
          <label className="field compact">
            <span>Shift Y</span>
            <input
              type="number"
              value={shiftY}
              step={1}
              onChange={(event) =>
                onShiftYChange(Number.parseInt(event.target.value, 10) || 0)
              }
            />
          </label>
        </div>
        <button
          type="button"
          className="btn full-width"
          onClick={onPasteShifted}
          disabled={!clipboard.hasData}
        >
          Paste with shift
        </button>
      </section>

      <section className="panel-section">
        <h2>View</h2>
        <label className="check-row">
          <input
            type="checkbox"
            checked={onionSkinning}
            onChange={(event) => onOnionSkinningChange(event.target.checked)}
          />
          <span>Onion skinning</span>
        </label>
        {onionSkinning && (
          <label className="field">
            <span>
              Opacity <strong>{Math.round(onionAlpha * 100)}%</strong>
            </span>
            <input
              type="range"
              min={0.1}
              max={0.5}
              step={0.05}
              value={onionAlpha}
              onChange={(event) =>
                onOnionAlphaChange(Number(event.target.value))
              }
            />
          </label>
        )}
      </section>
    </div>
  );
}
