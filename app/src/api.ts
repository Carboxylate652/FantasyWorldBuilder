// Transport to the Rust core: Tauri IPC inside the desktop app, or HTTP to
// `worldgen serve` when the UI runs in a plain browser.

export const isTauri = '__TAURI_INTERNALS__' in window;

const httpBase: string =
  (import.meta.env.VITE_API as string | undefined) ?? (location.port === '5173' ? 'http://127.0.0.1:8765' : '');

class ApiError extends Error {
  status?: number;
  constructor(message: string, status?: number) {
    super(message);
    this.status = status;
  }
}

async function raw(cmd: string, args: unknown = {}): Promise<ArrayBuffer> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    const res = await invoke<ArrayBuffer | number[]>('api', { cmd, args });
    return res instanceof ArrayBuffer ? res : new Uint8Array(res).buffer;
  }
  const r = await fetch(`${httpBase}/api/${cmd}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(args) });
  if (!r.ok) throw new ApiError(await r.text(), r.status);
  return r.arrayBuffer();
}

export async function json<T = any>(cmd: string, args: unknown = {}): Promise<T> {
  const b = await raw(cmd, args);
  return JSON.parse(new TextDecoder().decode(b)) as T;
}

export const bin = raw;

export type FieldData = {
  values: Float32Array | Uint8Array | Uint16Array | Uint32Array | Int32Array;
  stale: boolean;
  step: number;
};

function isMissingFieldError(err: unknown): boolean {
  if (err instanceof ApiError && err.status === 404) return true;
  const msg = err instanceof Error ? err.message : String(err);
  return /not available|not computed yet|no such field/i.test(msg);
}

export async function field(name: string, month?: number): Promise<FieldData | null> {
  try {
    const b = await raw('field', { name, month: month ?? null });
    if (b.byteLength < 8) throw new Error(`field "${name}" payload truncated (${b.byteLength} bytes)`);
    const dv = new DataView(b);
    const code = dv.getUint8(0);
    const stale = dv.getUint8(1) === 1;
    const step = dv.getUint16(2, true);
    const count = dv.getUint32(4, true);
    const elemSize = code === 1 ? 1 : code === 2 ? 2 : code === 0 || code === 3 || code === 4 ? 4 : 0;
    if (!elemSize) throw new Error(`field "${name}" has unknown type code ${code}`);
    if (b.byteLength < 8 + count * elemSize) {
      throw new Error(`field "${name}" payload too short (${b.byteLength} < ${8 + count * elemSize})`);
    }
    const values =
      code === 0 ? new Float32Array(b, 8, count)
      : code === 1 ? new Uint8Array(b, 8, count)
      : code === 2 ? new Uint16Array(b, 8, count)
      : code === 3 ? new Uint32Array(b, 8, count)
      : new Int32Array(b, 8, count);
    return { values, stale, step };
  } catch (err) {
    if (!isMissingFieldError(err)) console.error(`Failed to load field "${name}":`, err);
    return null;
  }
}

// ---- folder dialogs

export async function pickFolder(title: string, save: boolean): Promise<string | null> {
  if (isTauri) {
    const dlg = await import('@tauri-apps/plugin-dialog');
    if (save) {
      // A project is a folder; pick its parent, then name it.
      const p = await dlg.save({ title, defaultPath: 'my-world' });
      return p ?? null;
    }
    const p = await dlg.open({ title, directory: true, multiple: false });
    return typeof p === 'string' ? p : null;
  }
  return window.prompt(`${title}\nFolder path on the machine running worldgen serve:`, '') || null;
}

export async function pickFile(title: string, kind: string, extensions: string[]): Promise<string | null> {
  if (isTauri) {
    const dlg = await import('@tauri-apps/plugin-dialog');
    const p = await dlg.open({ title, multiple: false, filters: [{ name: kind, extensions }] });
    return typeof p === 'string' ? p : null;
  }
  return window.prompt(`${title}\n${extensions.map((e) => e.toUpperCase()).join('/')} path on the machine running worldgen serve:`, '') || null;
}

export const pickPng = (title: string) => pickFile(title, 'PNG image', ['png']);

/** A file to write (Tauri: save dialog; browser: a path on the machine running worldgen serve). */
export async function pickSaveFile(title: string, kind: string, extensions: string[], defaultPath: string): Promise<string | null> {
  if (isTauri) {
    const dlg = await import('@tauri-apps/plugin-dialog');
    const p = await dlg.save({ title, defaultPath, filters: [{ name: kind, extensions }] });
    return p ?? null;
  }
  return window.prompt(`${title}\nFile path on the machine running worldgen serve:`, defaultPath) || null;
}
