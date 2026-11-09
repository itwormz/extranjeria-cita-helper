/** Scrolling the page with the wheel must never change a focused number field or select: leave the field instead. */
export function guardWheel(event: {target: unknown}): boolean {
  const target = event.target as {tagName?: string; type?: string; blur?: () => void; ownerDocument?: {activeElement?: unknown}} | null;
  if (!target || typeof target.blur !== 'function') return false;
  const numeric = target.tagName === 'INPUT' && target.type === 'number';
  if (!numeric && target.tagName !== 'SELECT') return false;
  if (target.ownerDocument?.activeElement !== target) return false;
  target.blur();
  return true;
}
