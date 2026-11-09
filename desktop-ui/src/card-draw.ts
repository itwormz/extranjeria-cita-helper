import type {CardLayout, CardOp, CardTextOp} from './bridge';

/** The card is replayed from the Rust layout (identical to Python 0.3.6); nothing but public statistics gets here. */
export interface CardContext {
  fillStyle: string | CanvasGradient | CanvasPattern;
  strokeStyle: string | CanvasGradient | CanvasPattern;
  lineWidth: number;
  font: string;
  textAlign: CanvasTextAlign;
  textBaseline: CanvasTextBaseline;
  fillRect(x: number, y: number, w: number, h: number): void;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  arcTo(x1: number, y1: number, x2: number, y2: number, radius: number): void;
  closePath(): void;
  fill(): void;
  stroke(): void;
  save(): void;
  restore(): void;
  translate(x: number, y: number): void;
  rotate(angle: number): void;
  fillText(text: string, x: number, y: number): void;
  measureText(text: string): {width: number};
}
export type IconPainter = (name: string, x: number, y: number, size: number, color: string) => Promise<void> | void;

const FAMILY = 'Plex';
const MINIMUM_SIZE = 18;
const LINE_HEIGHT = 1.3;

function font(size: number, bold: boolean): string {
  return `${bold ? 600 : 400} ${size}px ${FAMILY}`;
}

/** Break `value` into lines no wider than `width`; a single word wider than that stays on its own line. */
export function wrapLines(ctx: Pick<CardContext, 'measureText'>, value: string, width: number): string[] {
  const lines: string[] = [];
  let current = '';
  for (const word of value.split(/\s+/).filter(Boolean)) {
    const candidate = current ? `${current} ${word}` : word;
    if (current && ctx.measureText(candidate).width > width) {
      lines.push(current);
      current = word;
    } else {
      current = candidate;
    }
  }
  if (current || !lines.length) lines.push(current);
  return lines;
}

/** Largest pixel size from `size` down to 18 at which the text fits the box (Python `_Card.text`). */
export function fitText(ctx: CardContext, op: CardTextOp): {size: number; lines: string[]} {
  let size = op.size;
  let lines = [op.value];
  for (; size >= MINIMUM_SIZE; size--) {
    ctx.font = font(size, op.bold);
    lines = op.wrap ? wrapLines(ctx, op.value, op.w) : [op.value];
    const widest = Math.max(...lines.map(line => ctx.measureText(line).width));
    if (widest <= op.w && lines.length * size * LINE_HEIGHT <= op.h) return {size, lines};
  }
  size = MINIMUM_SIZE;
  ctx.font = font(size, op.bold);
  return {size, lines: op.wrap ? wrapLines(ctx, op.value, op.w) : [op.value]};
}

function roundedRect(ctx: CardContext, x: number, y: number, w: number, h: number, radius: number): void {
  const r = Math.min(radius, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function drawText(ctx: CardContext, op: CardTextOp): void {
  const {size, lines} = fitText(ctx, op);
  ctx.fillStyle = op.color;
  ctx.textAlign = op.right ? 'right' : 'left';
  ctx.textBaseline = 'middle';
  const height = size * LINE_HEIGHT;
  const top = op.y + op.h / 2 - (lines.length * height) / 2;
  const x = op.right ? op.x + op.w : op.x;
  lines.forEach((line, index) => ctx.fillText(line, x, top + height * (index + 0.5)));
}

/** Replay every operation in order; `paintIcon` renders the stroke icons (SVG) for the current theme colour. */
export async function drawCard(ctx: CardContext, layout: CardLayout, paintIcon: IconPainter): Promise<void> {
  ctx.fillStyle = layout.background;
  ctx.fillRect(0, 0, layout.width, layout.height);
  for (const op of layout.ops as CardOp[]) {
    switch (op.op) {
      case 'box':
        roundedRect(ctx, op.x, op.y, op.w, op.h, op.radius);
        ctx.fillStyle = op.color;
        ctx.fill();
        if (op.border) {
          ctx.strokeStyle = op.border;
          ctx.lineWidth = 1;
          ctx.stroke();
        }
        break;
      case 'text':
        drawText(ctx, op);
        break;
      case 'line':
        ctx.strokeStyle = op.color;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(op.x, op.y);
        ctx.lineTo(op.x + op.w, op.y);
        ctx.stroke();
        break;
      case 'stroke':
        ctx.strokeStyle = op.color;
        ctx.lineWidth = op.width;
        ctx.beginPath();
        ctx.moveTo(op.x1, op.y1);
        ctx.lineTo(op.x2, op.y2);
        ctx.stroke();
        break;
      case 'icon':
        await paintIcon(op.name, op.x, op.y, op.size, op.color);
        break;
      case 'save':
        ctx.save();
        break;
      case 'restore':
        ctx.restore();
        break;
      case 'translate':
        ctx.translate(op.x, op.y);
        break;
      case 'rotate':
        ctx.rotate((op.angle * Math.PI) / 180);
        break;
    }
  }
}
