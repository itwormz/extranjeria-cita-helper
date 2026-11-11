/** Journal table: when a row selection is kept, and user-resizable column widths. */
export type JournalKind = 'timeline' | 'offices';
export const MIN_COLUMN = 60;
const MAX_COLUMN = 1200;
const DEFAULTS: Record<JournalKind, number[]> = {timeline: [90], offices: [90]};
/** The last column is not listed: it takes the remaining width. */
export function defaultWidths(kind: JournalKind): number[] {
  return [...DEFAULTS[kind]];
}
/** A key built from values, not from the polled snapshot object (a new object every half second). */
export function selectionScope(journal: string, profile: string | null | undefined, active: boolean | null | undefined): string {
  return `${journal}|${profile ?? ''}|${active ? 1 : 0}`;
}
export function resizeWidths(widths: number[], index: number, delta: number, min = MIN_COLUMN): number[] {
  if (!Number.isInteger(index) || index < 0 || index >= widths.length) return widths;
  const next = [...widths];
  next[index] = Math.min(MAX_COLUMN, Math.max(min, Math.round(widths[index] + delta)));
  return next;
}
const key = (kind: JournalKind) => `santacita.journal-columns.${kind}`;
type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'>;
/** Remembered widths are only a convenience: anything unexpected falls back to the defaults. */
export function loadWidths(kind: JournalKind, storage: Storage | undefined = globalThis.localStorage): number[] {
  try {
    const raw = storage?.getItem(key(kind));
    if (!raw) return defaultWidths(kind);
    const value: unknown = JSON.parse(raw);
    const expected = DEFAULTS[kind].length;
    if (Array.isArray(value) && value.length === expected && value.every(v => Number.isFinite(v) && v >= MIN_COLUMN && v <= MAX_COLUMN)) return value as number[];
  } catch {
    /* Storage can be blocked or hold junk. */
  }
  return defaultWidths(kind);
}
export function saveWidths(kind: JournalKind, widths: number[], storage: Storage | undefined = globalThis.localStorage): void {
  try {
    storage?.setItem(key(kind), JSON.stringify(widths));
  } catch {
    /* Remembering is optional. */
  }
}
