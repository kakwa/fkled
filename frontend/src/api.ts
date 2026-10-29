import type {
  Animation,
  AnimationPayload,
  FilePayload,
  FontPayload,
  HealthPayload,
} from './types';

const API_BASE = '/api';

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  const payload = (await response.json().catch(() => null)) as
    | { error?: string }
    | null;

  if (!response.ok) {
    throw new Error(payload?.error ?? `Request failed (${response.status})`);
  }
  return payload as T;
}

export function health(): Promise<HealthPayload> {
  return fetch(`${API_BASE}/health`, { cache: 'no-store' }).then((response) => {
    if (!response.ok) {
      throw new Error(`Server returned ${response.status}`);
    }
    return response.json() as Promise<HealthPayload>;
  });
}

export function importUsbFxd(data: string): Promise<AnimationPayload> {
  return postJson<AnimationPayload>('/import/usb-fxd', { data });
}

export function importGif(data: string): Promise<AnimationPayload> {
  return postJson<AnimationPayload>('/import/gif', { data });
}

export function exportUsbFxd(animation: Animation): Promise<FilePayload> {
  return postJson<FilePayload>('/export/usb-fxd', { animation });
}

export function exportGif(animation: Animation): Promise<FilePayload> {
  return postJson<FilePayload>('/export/gif', { animation });
}

export function parseFont(data: string): Promise<FontPayload> {
  return postJson<FontPayload>('/fonts/parse', { data });
}
