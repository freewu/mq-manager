#!/usr/bin/env node
/**
 * Generates every application icon from code — no binary assets in the repo and
 * no dependency on `tauri icon` (which would need a source PNG committed).
 *
 * Run directly (`node scripts/gen-icon.mjs`) or through `just icons`.
 *
 * Output (`src-tauri/icons/`):
 *   32x32.png, 128x128.png, 128x128@2x.png, icon.png, icon.ico, icon.icns
 */
import zlib from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT = join(ROOT, 'src-tauri', 'icons');

// ---------------------------------------------------------------------------
// Minimal PNG / ICO / ICNS encoders
// ---------------------------------------------------------------------------

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([length, body, crc]);
}

/** @param {Uint8Array} rgba row-major RGBA, `size * size * 4` */
function encodePng(size, rgba) {
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y += 1) {
    raw[y * (stride + 1)] = 0; // filter: none
    Buffer.from(rgba.buffer, rgba.byteOffset + y * stride, stride).copy(
      raw,
      y * (stride + 1) + 1,
    );
  }

  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // colour type: RGBA
  // 10..12 = compression, filter, interlace = 0

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

/** PNG-in-ICO (supported by every Windows since Vista). */
function encodeIco(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  let offset = 6 + images.length * 16;
  const entries = images.map(({ size, png }) => {
    const entry = Buffer.alloc(16);
    entry[0] = size >= 256 ? 0 : size;
    entry[1] = size >= 256 ? 0 : size;
    entry[2] = 0; // palette
    entry[3] = 0; // reserved
    entry.writeUInt16LE(1, 4); // colour planes
    entry.writeUInt16LE(32, 6); // bits per pixel
    entry.writeUInt32LE(png.length, 8);
    entry.writeUInt32LE(offset, 12);
    offset += png.length;
    return entry;
  });

  return Buffer.concat([header, ...entries, ...images.map((image) => image.png)]);
}

/** PNG-in-ICNS: `ic07`..`ic10` are the plain sizes, `ic11`..`ic14` the @2x ones. */
function encodeIcns(entries) {
  const chunks = entries.map(({ type, png }) => {
    const header = Buffer.alloc(8);
    header.write(type, 0, 'ascii');
    header.writeUInt32BE(png.length + 8, 4);
    return Buffer.concat([header, png]);
  });
  const body = Buffer.concat(chunks);
  const header = Buffer.alloc(8);
  header.write('icns', 0, 'ascii');
  header.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([header, body]);
}

// ---------------------------------------------------------------------------
// Artwork — designed on a 1024x1024 grid, rendered per size for crisp edges
// ---------------------------------------------------------------------------

const DESIGN = 1024;
const TOP_COLOR = [152, 217, 142]; // #98d98e
const BOTTOM_COLOR = [95, 201, 168]; // #5fc9a8

const BARS = [
  { left: 190, right: 520, centerY: 356 },
  { left: 190, right: 598, centerY: 512 },
  { left: 190, right: 452, centerY: 668 },
];
const BAR_HEIGHT = 92;

function sdRoundRect(px, py, cx, cy, halfW, halfH, radius) {
  const qx = Math.abs(px - cx) - (halfW - radius);
  const qy = Math.abs(py - cy) - (halfH - radius);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  return Math.min(Math.max(qx, qy), 0) + outside - radius;
}

function coverage(distance, feather) {
  return Math.min(Math.max(0.5 - distance / feather, 0), 1);
}

function render(size) {
  const rgba = new Uint8Array(size * size * 4);
  const scale = DESIGN / size;
  const feather = Math.max(scale, 0.75);
  const tileRadius = DESIGN * 0.22;
  const tileHalf = DESIGN / 2;

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const px = (x + 0.5) * scale;
      const py = (y + 0.5) * scale;

      const tileAlpha = coverage(
        sdRoundRect(px, py, tileHalf, tileHalf, tileHalf, tileHalf, tileRadius),
        feather,
      );

      if (tileAlpha <= 0) continue;

      const t = Math.min(Math.max((px + py) / (DESIGN * 2), 0), 1);
      let r = TOP_COLOR[0] + (BOTTOM_COLOR[0] - TOP_COLOR[0]) * t;
      let g = TOP_COLOR[1] + (BOTTOM_COLOR[1] - TOP_COLOR[1]) * t;
      let b = TOP_COLOR[2] + (BOTTOM_COLOR[2] - TOP_COLOR[2]) * t;

      let barAlpha = 0;
      for (const bar of BARS) {
        const halfW = (bar.right - bar.left) / 2;
        const centerX = (bar.right + bar.left) / 2;
        const distance = sdRoundRect(
          px,
          py,
          centerX,
          bar.centerY,
          halfW,
          BAR_HEIGHT / 2,
          BAR_HEIGHT / 2,
        );
        barAlpha = Math.max(barAlpha, coverage(distance, feather));
      }

      if (barAlpha > 0) {
        // Soft white “messages”, slightly translucent so the gradient shows through.
        const mix = barAlpha * 0.94;
        r = r + (255 - r) * mix;
        g = g + (255 - g) * mix;
        b = b + (255 - b) * mix;
      }

      const offset = (y * size + x) * 4;
      rgba[offset] = Math.round(r);
      rgba[offset + 1] = Math.round(g);
      rgba[offset + 2] = Math.round(b);
      rgba[offset + 3] = Math.round(tileAlpha * 255);
    }
  }

  return rgba;
}

// ---------------------------------------------------------------------------

function main() {
  mkdirSync(OUT, { recursive: true });

  const cache = new Map();
  const png = (size) => {
    if (!cache.has(size)) cache.set(size, encodePng(size, render(size)));
    return cache.get(size);
  };

  const files = [
    ['32x32.png', png(32)],
    ['128x128.png', png(128)],
    ['128x128@2x.png', png(256)],
    ['icon.png', png(512)],
    [
      'icon.ico',
      encodeIco([16, 24, 32, 48, 64, 128, 256].map((size) => ({ size, png: png(size) }))),
    ],
    [
      'icon.icns',
      encodeIcns([
        { type: 'ic07', png: png(128) },
        { type: 'ic08', png: png(256) },
        { type: 'ic09', png: png(512) },
        { type: 'ic10', png: png(1024) },
        { type: 'ic11', png: png(32) },
        { type: 'ic12', png: png(64) },
        { type: 'ic13', png: png(256) },
        { type: 'ic14', png: png(512) },
      ]),
    ],
  ];

  for (const [name, data] of files) {
    writeFileSync(join(OUT, name), data);
    console.log(`  icons/${name}  ${(data.length / 1024).toFixed(1)} KiB`);
  }
  console.log(`\nWrote ${files.length} icon files to src-tauri/icons`);
}

main();
