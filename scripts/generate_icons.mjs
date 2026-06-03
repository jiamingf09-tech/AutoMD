import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

const root = path.resolve("src-tauri/icons");
fs.mkdirSync(root, { recursive: true });

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n += 1) {
  let c = n;
  for (let k = 0; k < 8; k += 1) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  crcTable[n] = c >>> 0;
}

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) {
    c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, "ascii");
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBuffer.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 8 + data.length);
  return chunk;
}

function encodePng(width, height, rgba) {
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function blendPixel(buffer, size, x, y, color, alpha = 1) {
  const px = Math.round(x);
  const py = Math.round(y);
  if (px < 0 || py < 0 || px >= size || py >= size) return;
  const offset = (py * size + px) * 4;
  const a = Math.max(0, Math.min(1, alpha * (color[3] / 255)));
  buffer[offset] = Math.round(color[0] * a + buffer[offset] * (1 - a));
  buffer[offset + 1] = Math.round(color[1] * a + buffer[offset + 1] * (1 - a));
  buffer[offset + 2] = Math.round(color[2] * a + buffer[offset + 2] * (1 - a));
  buffer[offset + 3] = Math.round(255 * a + buffer[offset + 3] * (1 - a));
}

function drawCircle(buffer, size, cx, cy, radius, color) {
  const minX = Math.floor(cx - radius);
  const maxX = Math.ceil(cx + radius);
  const minY = Math.floor(cy - radius);
  const maxY = Math.ceil(cy + radius);
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const distance = Math.hypot(x + 0.5 - cx, y + 0.5 - cy);
      if (distance <= radius + 1) {
        blendPixel(buffer, size, x, y, color, Math.max(0, Math.min(1, radius + 1 - distance)));
      }
    }
  }
}

function drawLine(buffer, size, x0, y0, x1, y1, thickness, color) {
  const minX = Math.floor(Math.min(x0, x1) - thickness);
  const maxX = Math.ceil(Math.max(x0, x1) + thickness);
  const minY = Math.floor(Math.min(y0, y1) - thickness);
  const maxY = Math.ceil(Math.max(y0, y1) + thickness);
  const dx = x1 - x0;
  const dy = y1 - y0;
  const lengthSquared = dx * dx + dy * dy;

  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const t = Math.max(
        0,
        Math.min(1, ((x + 0.5 - x0) * dx + (y + 0.5 - y0) * dy) / lengthSquared),
      );
      const nearestX = x0 + t * dx;
      const nearestY = y0 + t * dy;
      const distance = Math.hypot(x + 0.5 - nearestX, y + 0.5 - nearestY);
      if (distance <= thickness / 2 + 1) {
        blendPixel(buffer, size, x, y, color, Math.max(0, Math.min(1, thickness / 2 + 1 - distance)));
      }
    }
  }
}

function drawIcon(size) {
  const buffer = Buffer.alloc(size * size * 4);
  const cornerRadius = size * 0.2;

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const dx = Math.max(cornerRadius - x, 0, x - (size - cornerRadius));
      const dy = Math.max(cornerRadius - y, 0, y - (size - cornerRadius));
      if (Math.hypot(dx, dy) > cornerRadius) continue;

      const gx = x / (size - 1);
      const gy = y / (size - 1);
      const glow = Math.exp(-((gx - 0.72) ** 2 + (gy - 0.28) ** 2) / 0.055);
      const offset = (y * size + x) * 4;
      buffer[offset] = Math.round(14 + 35 * gy + 18 * glow);
      buffer[offset + 1] = Math.round(47 + 92 * (1 - gy) + 55 * glow);
      buffer[offset + 2] = Math.round(54 + 80 * gx + 50 * glow);
      buffer[offset + 3] = 255;
    }
  }

  const s = size / 1024;
  const bondColor = [226, 252, 244, 200];
  drawLine(buffer, size, 292 * s, 646 * s, 474 * s, 454 * s, 32 * s, bondColor);
  drawLine(buffer, size, 474 * s, 454 * s, 684 * s, 568 * s, 32 * s, bondColor);
  drawLine(buffer, size, 474 * s, 454 * s, 626 * s, 300 * s, 26 * s, bondColor);
  drawLine(buffer, size, 626 * s, 300 * s, 776 * s, 344 * s, 24 * s, bondColor);
  drawLine(buffer, size, 310 * s, 770 * s, 748 * s, 770 * s, 18 * s, [126, 226, 202, 150]);

  const atoms = [
    [292 * s, 646 * s, 88 * s, [238, 252, 246, 255]],
    [474 * s, 454 * s, 72 * s, [107, 226, 186, 255]],
    [684 * s, 568 * s, 82 * s, [92, 183, 255, 255]],
    [626 * s, 300 * s, 62 * s, [255, 226, 128, 255]],
    [776 * s, 344 * s, 46 * s, [255, 255, 255, 245]],
  ];

  for (const [cx, cy, radius, color] of atoms) {
    drawCircle(buffer, size, cx, cy, radius * 1.18, [0, 0, 0, 55]);
    drawCircle(buffer, size, cx, cy, radius, color);
    drawCircle(buffer, size, cx - radius * 0.32, cy - radius * 0.36, radius * 0.22, [255, 255, 255, 190]);
  }

  return encodePng(size, size, buffer);
}

const pngs = new Map();
for (const size of [16, 32, 64, 128, 256, 512, 1024]) {
  pngs.set(size, drawIcon(size));
}

fs.writeFileSync(path.join(root, "icon.png"), pngs.get(1024));
fs.writeFileSync(path.join(root, "32x32.png"), pngs.get(32));
fs.writeFileSync(path.join(root, "128x128.png"), pngs.get(128));
fs.writeFileSync(path.join(root, "128x128@2x.png"), pngs.get(256));

const icoEntries = [
  { size: 32, png: pngs.get(32) },
  { size: 256, png: pngs.get(256) },
];
let offset = 6 + icoEntries.length * 16;
const icoHeader = Buffer.alloc(offset);
icoHeader.writeUInt16LE(0, 0);
icoHeader.writeUInt16LE(1, 2);
icoHeader.writeUInt16LE(icoEntries.length, 4);
icoEntries.forEach((entry, index) => {
  const base = 6 + index * 16;
  icoHeader[base] = entry.size === 256 ? 0 : entry.size;
  icoHeader[base + 1] = entry.size === 256 ? 0 : entry.size;
  icoHeader.writeUInt16LE(1, base + 4);
  icoHeader.writeUInt16LE(32, base + 6);
  icoHeader.writeUInt32LE(entry.png.length, base + 8);
  icoHeader.writeUInt32LE(offset, base + 12);
  offset += entry.png.length;
});
fs.writeFileSync(path.join(root, "icon.ico"), Buffer.concat([icoHeader, ...icoEntries.map((entry) => entry.png)]));

const iconset = path.join(root, "icon.iconset");
fs.mkdirSync(iconset, { recursive: true });
const iconsetFiles = [
  ["icon_16x16.png", 16],
  ["icon_16x16@2x.png", 32],
  ["icon_32x32.png", 32],
  ["icon_32x32@2x.png", 64],
  ["icon_128x128.png", 128],
  ["icon_128x128@2x.png", 256],
  ["icon_256x256.png", 256],
  ["icon_256x256@2x.png", 512],
  ["icon_512x512.png", 512],
  ["icon_512x512@2x.png", 1024],
];
for (const [name, size] of iconsetFiles) {
  fs.writeFileSync(path.join(iconset, name), pngs.get(size));
}

console.log(`Generated AutoMD icons in ${root}`);
