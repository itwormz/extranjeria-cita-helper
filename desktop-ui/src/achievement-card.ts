import type {CardLayout} from './bridge';
import paths from './icons.json';
import {drawCard} from './card-draw';

export function iconSvg(name: string, color: string): string {
  const body = (paths as Record<string, string>)[name] ?? paths.list;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="${color}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
}

export async function renderCard(layout: CardLayout): Promise<HTMLCanvasElement> {
  await document.fonts.ready;
  const canvas = document.createElement('canvas');
  canvas.width = layout.width;
  canvas.height = layout.height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas_unavailable');
  await drawCard(ctx, layout, async (name, x, y, size, color) => {
    const image = new Image();
    image.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(iconSvg(name, color));
    await image.decode();
    ctx.drawImage(image, x, y, size, size);
  });
  return canvas;
}

export async function pngBytes(canvas: HTMLCanvasElement): Promise<number[]> {
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(value => (value ? resolve(value) : reject(new Error('png_unavailable'))), 'image/png'));
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}
