/** Text for the clipboard from the journal table as shown (Python `copy_log`): headers, then rows, tab separated. */
export function journalText(headers: string[], rows: string[][], picked: number[]): string {
  const chosen = picked.length ? [...new Set(picked)].filter(i => i >= 0 && i < rows.length).sort((a, b) => a - b) : rows.map((_, i) => i);
  const clean = (cell: string) => cell.replace(/[\t\r\n]+/g, ' ');
  return [headers, ...chosen.map(i => rows[i])].map(line => line.map(clean).join('\t')).join('\n');
}
/** Rows chosen by a click: plain selects one, Cmd/Ctrl toggles, Shift extends from the anchor. */
export function nextSelection(current: number[], index: number, anchor: number | null, mode: {toggle: boolean; range: boolean}): number[] {
  if (mode.range && anchor !== null) {
    const [from, to] = anchor < index ? [anchor, index] : [index, anchor];
    return Array.from({length: to - from + 1}, (_, i) => from + i);
  }
  if (mode.toggle) return current.includes(index) ? current.filter(i => i !== index) : [...current, index];
  return current.length === 1 && current[0] === index ? [] : [index];
}
