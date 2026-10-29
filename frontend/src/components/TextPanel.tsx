import type { ChangeEvent } from 'react';
import { PALETTE } from '../palette';
import type { PcfFont } from '../types';

type Props = {
  font: PcfFont | null;
  fontName: string | null;
  onLoadFont: (file: File) => void;
  text: string;
  onTextChange: (value: string) => void;
  textX: number;
  textY: number;
  textColor: number;
  onTextXChange: (value: number) => void;
  onTextYChange: (value: number) => void;
  onTextColorChange: (value: number) => void;
  onRenderText: () => void;
  scrollText: string;
  scrollSpeed: number;
  scrollGap: number;
  onScrollTextChange: (value: string) => void;
  onScrollSpeedChange: (value: number) => void;
  onScrollGapChange: (value: number) => void;
  onGenerateScroll: () => void;
};

export function TextPanel({
  font,
  fontName,
  onLoadFont,
  text,
  onTextChange,
  textX,
  textY,
  textColor,
  onTextXChange,
  onTextYChange,
  onTextColorChange,
  onRenderText,
  scrollText,
  scrollSpeed,
  scrollGap,
  onScrollTextChange,
  onScrollSpeedChange,
  onScrollGapChange,
  onGenerateScroll,
}: Props) {
  function handleFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (file) {
      onLoadFont(file);
    }
    event.target.value = '';
  }

  return (
    <div className="panel-stack">
      <section className="panel-section">
        <h2>Text &amp; fonts</h2>
        <label className="btn file-button full-width">
          Load PCF font (.pcf / .pcf.gz)
          <input type="file" accept=".pcf,.gz,application/gzip" onChange={handleFile} />
        </label>
        <p className={`font-status ${font ? 'loaded' : ''}`}>
          {font
            ? `${fontName ?? 'Font'} — ${Object.keys(font.glyphs).length} glyphs`
            : 'No font loaded'}
        </p>
      </section>

      <section className="panel-section">
        <h2>Insert text</h2>
        <label className="field">
          <span>Text</span>
          <input
            type="text"
            value={text}
            placeholder="HELLO"
            onChange={(event) => onTextChange(event.target.value)}
          />
        </label>
        <div className="field-grid">
          <label className="field compact">
            <span>X</span>
            <input
              type="number"
              value={textX}
              onChange={(event) =>
                onTextXChange(Number.parseInt(event.target.value, 10) || 0)
              }
            />
          </label>
          <label className="field compact">
            <span>Y</span>
            <input
              type="number"
              value={textY}
              onChange={(event) =>
                onTextYChange(Number.parseInt(event.target.value, 10) || 0)
              }
            />
          </label>
          <label className="field compact">
            <span>Colour</span>
            <select
              value={textColor}
              onChange={(event) =>
                onTextColorChange(Number(event.target.value))
              }
            >
              {PALETTE.map((entry) => (
                <option key={entry.index} value={entry.index}>
                  {entry.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <button
          type="button"
          className="btn primary full-width"
          disabled={!font}
          onClick={onRenderText}
        >
          Render text to frame
        </button>
      </section>

      <section className="panel-section">
        <h2>Scrolling marquee</h2>
        <label className="field">
          <span>Text</span>
          <input
            type="text"
            value={scrollText}
            placeholder="SCROLLING TEXT"
            onChange={(event) => onScrollTextChange(event.target.value)}
          />
        </label>
        <div className="field-grid">
          <label className="field compact">
            <span>Speed (px)</span>
            <input
              type="number"
              min={1}
              value={scrollSpeed}
              onChange={(event) =>
                onScrollSpeedChange(
                  Math.max(1, Number.parseInt(event.target.value, 10) || 1),
                )
              }
            />
          </label>
          <label className="field compact">
            <span>Gap (spaces)</span>
            <input
              type="number"
              min={0}
              value={scrollGap}
              onChange={(event) =>
                onScrollGapChange(
                  Math.max(0, Number.parseInt(event.target.value, 10) || 0),
                )
              }
            />
          </label>
        </div>
        <button
          type="button"
          className="btn full-width"
          disabled={!font || scrollText.length === 0}
          onClick={onGenerateScroll}
        >
          Generate scrolling frames
        </button>
        <p className="hint">
          Frames are generated in the browser from PCF glyphs returned by the
          server.
        </p>
      </section>
    </div>
  );
}
