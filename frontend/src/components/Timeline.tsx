import type { Frame } from '../types';
import { FrameThumbnail } from './FrameThumbnail';

type Props = {
  frames: Frame[];
  currentFrame: number;
  playing: boolean;
  onSelect: (index: number) => void;
  onFirst: () => void;
  onPrevious: () => void;
  onNext: () => void;
  onLast: () => void;
  onTogglePlay: () => void;
  onAdd: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onDurationChange: (durationMs: number) => void;
};

export function Timeline({
  frames,
  currentFrame,
  playing,
  onSelect,
  onFirst,
  onPrevious,
  onNext,
  onLast,
  onTogglePlay,
  onAdd,
  onDuplicate,
  onDelete,
  onDurationChange,
}: Props) {
  const frame = frames[currentFrame];

  return (
    <footer className="timeline">
      <div className="timeline-controls">
        <div className="transport">
          <button
            type="button"
            className="btn icon-button"
            onClick={onFirst}
            title="First frame"
          >
            ⏮
          </button>
          <button
            type="button"
            className="btn icon-button"
            onClick={onPrevious}
            title="Previous frame"
          >
            ◀
          </button>
          <button
            type="button"
            className={`btn icon-button play ${playing ? 'active' : ''}`}
            onClick={onTogglePlay}
            title={playing ? 'Pause' : 'Play'}
          >
            {playing ? '⏸' : '▶'}
          </button>
          <button
            type="button"
            className="btn icon-button"
            onClick={onNext}
            title="Next frame"
          >
            ▶
          </button>
          <button
            type="button"
            className="btn icon-button"
            onClick={onLast}
            title="Last frame"
          >
            ⏭
          </button>
        </div>

        <span className="frame-counter">
          Frame <strong>{currentFrame + 1}</strong> / {frames.length}
        </span>

        <div className="timeline-actions">
          <button type="button" className="btn" onClick={onAdd}>
            + Add
          </button>
          <button type="button" className="btn" onClick={onDuplicate}>
            Duplicate
          </button>
          <button
            type="button"
            className="btn danger"
            onClick={onDelete}
            disabled={frames.length <= 1}
          >
            Delete
          </button>
          <label className="duration-field">
            <span>Duration</span>
            <input
              type="number"
              min={10}
              max={1000}
              step={10}
              value={frame?.duration_ms ?? 100}
              onChange={(event) =>
                onDurationChange(
                  Math.min(
                    1000,
                    Math.max(10, Number.parseInt(event.target.value, 10) || 10),
                  ),
                )
              }
            />
            <span>ms</span>
          </label>
        </div>
      </div>

      <div className="frames-strip">
        {frames.map((item, index) => (
          <button
            key={index}
            type="button"
            className={`frame-button ${index === currentFrame ? 'active' : ''}`}
            onClick={() => onSelect(index)}
            title={`Frame ${index + 1} — ${item.duration_ms} ms`}
          >
            <FrameThumbnail pixels={item.pixels} />
            <span>{index + 1}</span>
          </button>
        ))}
      </div>
    </footer>
  );
}
