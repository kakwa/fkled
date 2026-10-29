import type { ChangeEvent } from 'react';
import type { Notice } from '../types';

export type ServerStatus = 'checking' | 'online' | 'offline';

type Props = {
  name: string;
  onNameChange: (value: string) => void;
  serverStatus: ServerStatus;
  busy: boolean;
  notice: Notice | null;
  onDismissNotice: () => void;
  onNew: () => void;
  onOpenProject: (file: File) => void;
  onSaveProject: () => void;
  onImportFxd: (file: File) => void;
  onExportFxd: () => void;
  onImportGif: (file: File) => void;
  onExportGif: () => void;
};

function pickFile(
  event: ChangeEvent<HTMLInputElement>,
  callback: (file: File) => void,
) {
  const file = event.target.files?.[0];
  if (file) {
    callback(file);
  }
  event.target.value = '';
}

export function MenuBar({
  name,
  onNameChange,
  serverStatus,
  busy,
  notice,
  onDismissNotice,
  onNew,
  onOpenProject,
  onSaveProject,
  onImportFxd,
  onExportFxd,
  onImportGif,
  onExportGif,
}: Props) {
  return (
    <header className="topbar">
      <div className="topbar-main">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span>
            LEDFX <em>Editor</em>
          </span>
        </div>

        <label className="project-name">
          <span>Project</span>
          <input
            type="text"
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            aria-label="Project name"
          />
        </label>

        <div className="topbar-spacer" />

        {busy && <span className="busy-indicator">Working…</span>}
        <span className={`server-status ${serverStatus}`}>
          <span className="status-dot" />
          {serverStatus === 'online'
            ? 'Server online'
            : serverStatus === 'offline'
              ? 'Server offline'
              : 'Connecting…'}
        </span>
      </div>

      <div className="toolbar">
        <div className="toolbar-group">
          <span className="toolbar-label">Project</span>
          <button type="button" className="btn" onClick={onNew}>
            New
          </button>
          <label className="btn file-button">
            Open JSON
            <input
              type="file"
              accept=".json,application/json"
              onChange={(event) => pickFile(event, onOpenProject)}
            />
          </label>
          <button type="button" className="btn" onClick={onSaveProject}>
            Save JSON
          </button>
        </div>

        <div className="toolbar-group">
          <span className="toolbar-label">USB.FXD</span>
          <label className="btn file-button">
            Import
            <input
              type="file"
              accept=".FXD,.fxd,application/octet-stream"
              onChange={(event) => pickFile(event, onImportFxd)}
            />
          </label>
          <button
            type="button"
            className="btn primary"
            onClick={onExportFxd}
            disabled={busy}
          >
            Export (server)
          </button>
        </div>

        <div className="toolbar-group">
          <span className="toolbar-label">GIF</span>
          <label className="btn file-button">
            Import
            <input
              type="file"
              accept=".gif,image/gif"
              onChange={(event) => pickFile(event, onImportGif)}
            />
          </label>
          <button
            type="button"
            className="btn"
            onClick={onExportGif}
            disabled={busy}
          >
            Export (server)
          </button>
        </div>
      </div>

      {notice && (
        <div className={`notice ${notice.kind}`} role="status">
          <span>{notice.text}</span>
          <button
            type="button"
            className="notice-close"
            onClick={onDismissNotice}
            aria-label="Dismiss message"
          >
            ×
          </button>
        </div>
      )}
    </header>
  );
}
