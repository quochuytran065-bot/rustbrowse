#!/usr/bin/env node
/**
 * RustBrowse Cute 2D Chibi Pet Mascot Icon Generator (Zero-Dependency Node.js)
 * Generates:
 *   - assets/pet_icon.png (512x512)
 *   - src-tauri/icons/icon.png (512x512)
 *   - src-tauri/icons/128x128@2x.png (256x256)
 *   - src-tauri/icons/128x128.png (128x128)
 *   - src-tauri/icons/32x32.png (32x32)
 *   - src-tauri/icons/icon.ico (multi-size: 16, 24, 32, 48, 64, 128, 256)
 */

import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');

// ---- Math & SDF Helpers ----
function clamp(v, lo, hi) { return v < lo ? lo : v > hi ? hi : v; }
function lerp(a, b, t) { return a + (b - a) * t; }
function smoothstep(e0, e1, x) {
  const t = clamp((x - e0) / (e1 - e0), 0, 1);
  return t * t * (3 - 2 * t);
}

// Signed distance to rounded superellipse (squircle)
function sdSquircle(x, y, r, n = 4.2) {
  const ax = Math.abs(x) / r;
  const ay = Math.abs(y) / r;
  const d = Math.pow(Math.pow(ax, n) + Math.pow(ay, n), 1 / n) - 1;
  return d * r;
}

// Signed distance to circle
function sdCircle(x, y, cx, cy, r) {
  return Math.hypot(x - cx, y - cy) - r;
}

// Signed distance to ellipse (approx, accurate near boundary)
function sdEllipse(x, y, cx, cy, rx, ry) {
  const dx = (x - cx) / rx;
  const dy = (y - cy) / ry;
  const k0 = Math.hypot(dx, dy);
  if (k0 === 0) return -Math.min(rx, ry);
  const k1 = Math.hypot(dx / rx, dy / ry);
  return (k0 * (k0 - 1)) / k1;
}

// Signed distance to segment (capsule)
function sdSegment(px, py, ax, ay, bx, by) {
  const pax = px - ax, pay = py - ay;
  const bax = bx - ax, bay = by - ay;
  const h = clamp((pax * bax + pay * bay) / (bax * bax + bay * bay), 0, 1);
  return Math.hypot(pax - bax * h, pay - bay * h);
}

// Signed distance to triangle
function sdTriangle(px, py, ax, ay, bx, by, cx, cy) {
  const e0x = bx - ax, e0y = by - ay;
  const e1x = cx - bx, e1y = cy - by;
  const e2x = ax - cx, e2y = ay - cy;
  const v0x = px - ax, v0y = py - ay;
  const v1x = px - bx, v1y = py - by;
  const v2x = px - cx, v2y = py - cy;

  const t0 = clamp((v0x * e0x + v0y * e0y) / (e0x * e0x + e0y * e0y), 0, 1);
  const t1 = clamp((v1x * e1x + v1y * e1y) / (e1x * e1x + e1y * e1y), 0, 1);
  const t2 = clamp((v2x * e2x + v2y * e2y) / (e2x * e2x + e2y * e2y), 0, 1);

  const pq0x = v0x - e0x * t0, pq0y = v0y - e0y * t0;
  const pq1x = v1x - e1x * t1, pq1y = v1y - e1y * t1;
  const pq2x = v2x - e2x * t2, pq2y = v2y - e2y * t2;

  const s = Math.sign(e0x * e2y - e0y * e2x) || 1;
  const d0 = pq0x * pq0x + pq0y * pq0y;
  const d1 = pq1x * pq1x + pq1y * pq1y;
  const d2 = pq2x * pq2x + pq2y * pq2y;
  const c0 = s * (v0x * e0y - v0y * e0x);
  const c1 = s * (v1x * e1y - v1y * e1x);
  const c2 = s * (v2x * e2y - v2y * e2x);

  const dist = Math.sqrt(Math.min(d0, d1, d2));
  const inside = Math.min(c0, c1, c2) > 0;
  return inside ? -dist : dist;
}

// Signed distance to arc (centered at cx, cy, radius r, between angle a0 and a1)
function sdArc(x, y, cx, cy, r, a0, a1, thickness) {
  const dx = x - cx, dy = y - cy;
  let ang = Math.atan2(dy, dx);
  const mid = (a0 + a1) * 0.5;
  const half = (a1 - a0) * 0.5;
  let diff = ang - mid;
  while (diff > Math.PI) diff -= Math.PI * 2;
  while (diff < -Math.PI) diff += Math.PI * 2;
  if (Math.abs(diff) <= half) {
    return Math.abs(Math.hypot(dx, dy) - r) - thickness;
  }
  const ex0 = cx + Math.cos(a0) * r, ey0 = cy + Math.sin(a0) * r;
  const ex1 = cx + Math.cos(a1) * r, ey1 = cy + Math.sin(a1) * r;
  return Math.min(Math.hypot(x - ex0, y - ey0), Math.hypot(x - ex1, y - ey1)) - thickness;
}

// Alpha-over compositing (straight RGBA in 0..1)
function blendOver(dst, r, g, b, a) {
  if (a <= 0) return;
  const outA = a + dst[3] * (1 - a);
  if (outA <= 0) return;
  dst[0] = (r * a + dst[0] * dst[3] * (1 - a)) / outA;
  dst[1] = (g * a + dst[1] * dst[3] * (1 - a)) / outA;
  dst[2] = (b * a + dst[2] * dst[3] * (1 - a)) / outA;
  dst[3] = outA;
}

function hex(h) {
  const n = parseInt(h.slice(1), 16);
  return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

const C = {
  bgTop: hex('#d9fbf5'),
  bgMid: hex('#8ce9db'),
  bgBot: hex('#38c6bd'),
  rimLight: hex('#ffffff'),
  outline: hex('#163142'),
  bandMain: hex('#189c9b'),
  bandHi: hex('#56e3d8'),
  cupOuter: hex('#ff7b9c'),
  cupHi: hex('#ffb3c6'),
  cupPad: hex('#1e3d52'),
  furMain: hex('#f7fffd'),
  furTint: hex('#d7f7f0'),
  furShadow: hex('#b5ebe0'),
  earOuter: hex('#7ce5d7'),
  earInner: hex('#ff9ebb'),
  eyeDark: hex('#122636'),
  eyeIris: hex('#1b6d82'),
  blush: hex('#ff7fa8'),
  nose: hex('#ff5c8a'),
  sparkle: hex('#ffffff'),
};

function sampleMascot(x, y, pxSize) {
  const aa = pxSize * 1.1;
  const col = [0, 0, 0, 0];

  // 1. Soft pastel mint-teal squircle backdrop (transparent outside)
  const dSquircle = sdSquircle(x, y, 0.91, 4.4);
  if (dSquircle < aa) {
    const mask = 1 - smoothstep(-aa, aa, dSquircle);
    const tY = clamp((y + 0.91) / 1.82, 0, 1);
    const r = tY < 0.5 ? lerp(C.bgTop[0], C.bgMid[0], tY * 2) : lerp(C.bgMid[0], C.bgBot[0], (tY - 0.5) * 2);
    const g = tY < 0.5 ? lerp(C.bgTop[1], C.bgMid[1], tY * 2) : lerp(C.bgMid[1], C.bgBot[1], (tY - 0.5) * 2);
    const b = tY < 0.5 ? lerp(C.bgTop[2], C.bgMid[2], tY * 2) : lerp(C.bgMid[2], C.bgBot[2], (tY - 0.5) * 2);
    blendOver(col, r, g, b, mask);

    // Subtle top glossy arc
    const dGloss = sdEllipse(x, y, 0, -0.58, 0.72, 0.34);
    const glossA = (1 - smoothstep(-aa, aa, dGloss)) * mask * 0.28;
    blendOver(col, 1, 1, 1, glossA);

    // Subtle inner rim highlight
    const rimA = smoothstep(-0.045, -0.012, dSquircle) * (1 - smoothstep(-0.012, 0.0, dSquircle)) * mask * 0.45;
    blendOver(col, 1, 1, 1, rimA);
  }

  // Soft shadow beneath chibi head
  const dShadow = sdEllipse(x, y, 0, 0.61, 0.50, 0.14);
  if (dShadow < 0.14) {
    const shA = (1 - smoothstep(-0.08, 0.12, dShadow)) * 0.18 * (1 - smoothstep(-aa, aa, dSquircle));
    blendOver(col, 0.05, 0.22, 0.28, shA);
  }

  // 2. Headphone arch band (behind ears/head)
  const dBand = sdArc(x, y, 0, 0.08, 0.64, -Math.PI * 0.92, -Math.PI * 0.08, 0.068);
  const dBandOutline = dBand - 0.028;
  if (dBandOutline < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], 1 - smoothstep(-aa, aa, dBandOutline));
    const fillA = 1 - smoothstep(-aa, aa, dBand);
    const hiT = clamp((y + 0.60) / 0.25, 0, 1);
    blendOver(col,
      lerp(C.bandHi[0], C.bandMain[0], hiT),
      lerp(C.bandHi[1], C.bandMain[1], hiT),
      lerp(C.bandHi[2], C.bandMain[2], hiT),
      fillA
    );
  }

  // 3. Pointed cute fox/cat ears (rounded triangles)
  const dEarL = sdTriangle(x, y, -0.46, -0.10, -0.41, -0.56, -0.08, -0.34) - 0.045;
  const dEarR = sdTriangle(x, y, 0.46, -0.10, 0.41, -0.56, 0.08, -0.34) - 0.045;
  const dEars = Math.min(dEarL, dEarR);
  if (dEars - 0.03 < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], 1 - smoothstep(-aa, aa, dEars - 0.03));
    blendOver(col, C.earOuter[0], C.earOuter[1], C.earOuter[2], 1 - smoothstep(-aa, aa, dEars));
  }
  const dEarInL = sdTriangle(x, y, -0.39, -0.18, -0.37, -0.46, -0.17, -0.33) - 0.02;
  const dEarInR = sdTriangle(x, y, 0.39, -0.18, 0.37, -0.46, 0.17, -0.33) - 0.02;
  const dEarsIn = Math.min(dEarInL, dEarInR);
  if (dEarsIn < aa) {
    blendOver(col, C.earInner[0], C.earInner[1], C.earInner[2], 1 - smoothstep(-aa, aa, dEarsIn));
  }

  // 4. Headphone ear cups (left & right — iconic 16px silhouette!)
  const dCupL = sdEllipse(x, y, -0.59, 0.14, 0.15, 0.22);
  const dCupR = sdEllipse(x, y, 0.59, 0.14, 0.15, 0.22);
  const dCups = Math.min(dCupL, dCupR);
  if (dCups - 0.032 < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], 1 - smoothstep(-aa, aa, dCups - 0.032));
    const cupA = 1 - smoothstep(-aa, aa, dCups);
    const cupHi = clamp((y + 0.08) / 0.42, 0, 1);
    blendOver(col,
      lerp(C.cupHi[0], C.cupOuter[0], cupHi),
      lerp(C.cupHi[1], C.cupOuter[1], cupHi),
      lerp(C.cupHi[2], C.cupOuter[2], cupHi),
      cupA
    );
    // Ear cup cushion inner strip
    const dPadL = sdEllipse(x, y, -0.50, 0.14, 0.06, 0.17);
    const dPadR = sdEllipse(x, y, 0.50, 0.14, 0.06, 0.17);
    blendOver(col, C.cupPad[0], C.cupPad[1], C.cupPad[2], (1 - smoothstep(-aa, aa, Math.min(dPadL, dPadR))) * cupA);
  }

  // 5. Chubby chibi head (rounded squircle-oval)
  const headY = y - 0.14;
  const dHead = sdEllipse(x, headY, 0, 0, 0.54, 0.47);
  if (dHead - 0.034 < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], 1 - smoothstep(-aa, aa, dHead - 0.034));
    const headA = 1 - smoothstep(-aa, aa, dHead);
    // Soft mint-cream fur gradient
    const furT = clamp((y + 0.35) / 0.95, 0, 1);
    blendOver(col,
      lerp(C.furMain[0], C.furTint[0], furT),
      lerp(C.furMain[1], C.furTint[1], furT),
      lerp(C.furMain[2], C.furTint[2], furT),
      headA
    );

    // Cute pastel mint crown marking on forehead
    const dMark = sdEllipse(x, y, 0, -0.34, 0.28, 0.18);
    blendOver(col, C.earOuter[0], C.earOuter[1], C.earOuter[2], (1 - smoothstep(-aa, aa, dMark)) * headA * 0.55);

    // Lower chin soft shadow
    const dChin = sdEllipse(x, y, 0, 0.62, 0.48, 0.16);
    blendOver(col, C.furShadow[0], C.furShadow[1], C.furShadow[2], (1 - smoothstep(-aa, aa, dChin)) * headA * 0.45);
  }

  // 6. Rosy pink blush cheeks
  const dBlushL = sdEllipse(x, y, -0.31, 0.28, 0.11, 0.065);
  const dBlushR = sdEllipse(x, y, 0.31, 0.28, 0.11, 0.065);
  const dBlush = Math.min(dBlushL, dBlushR);
  if (dBlush < 0.04) {
    const blushA = (1 - smoothstep(-0.02, 0.04, dBlush)) * 0.68;
    blendOver(col, C.blush[0], C.blush[1], C.blush[2], blushA);
  }

  // 7. Big expressive chibi eyes + iris glow + twin white sparkles
  for (const side of [-1, 1]) {
    const ex = side * 0.21;
    const ey = 0.11;
    const dEye = sdEllipse(x, y, ex, ey, 0.105, 0.125);
    if (dEye < aa) {
      const eyeA = 1 - smoothstep(-aa, aa, dEye);
      blendOver(col, C.eyeDark[0], C.eyeDark[1], C.eyeDark[2], eyeA);
      // Lower iris teal gleam
      const dIris = sdEllipse(x, y, ex, ey + 0.055, 0.075, 0.055);
      blendOver(col, C.eyeIris[0], C.eyeIris[1], C.eyeIris[2], (1 - smoothstep(-aa, aa, dIris)) * eyeA * 0.85);
      // Main sparkle top-left
      const dSp1 = sdCircle(x, y, ex - 0.035, ey - 0.042, 0.036);
      blendOver(col, 1, 1, 1, (1 - smoothstep(-aa, aa, dSp1)) * eyeA);
      // Secondary tiny sparkle bottom-right
      const dSp2 = sdCircle(x, y, ex + 0.032, ey + 0.036, 0.017);
      blendOver(col, 1, 1, 1, (1 - smoothstep(-aa, aa, dSp2)) * eyeA * 0.92);
    }
  }

  // 8. Cute button nose
  const dNose = sdEllipse(x, y, 0, 0.22, 0.042, 0.030);
  if (dNose < aa) {
    blendOver(col, C.nose[0], C.nose[1], C.nose[2], 1 - smoothstep(-aa, aa, dNose));
  }

  // 9. Happy 'w' chibi cat smile
  const dMouthL = sdArc(x, y, -0.055, 0.265, 0.055, 0.12 * Math.PI, 0.88 * Math.PI, 0.018);
  const dMouthR = sdArc(x, y, 0.055, 0.265, 0.055, 0.12 * Math.PI, 0.88 * Math.PI, 0.018);
  const dMouth = Math.min(dMouthL, dMouthR);
  if (dMouth < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], 1 - smoothstep(-aa, aa, dMouth));
  }

  // 10. Cute little whiskers
  const w1 = sdSegment(x, y, -0.42, 0.22, -0.50, 0.20) - 0.012;
  const w2 = sdSegment(x, y, -0.41, 0.28, -0.49, 0.29) - 0.012;
  const w3 = sdSegment(x, y, 0.42, 0.22, 0.50, 0.20) - 0.012;
  const w4 = sdSegment(x, y, 0.41, 0.28, 0.49, 0.29) - 0.012;
  const dWhisk = Math.min(w1, w2, w3, w4);
  if (dWhisk < aa) {
    blendOver(col, C.outline[0], C.outline[1], C.outline[2], (1 - smoothstep(-aa, aa, dWhisk)) * 0.65);
  }

  return col;
}

// Render RGBA buffer at size x size with 2x2 supersampling
function renderRGBA(size) {
  const buf = new Uint8Array(size * size * 4);
  const pxSize = 2.0 / size;
  const sub = [-0.25, 0.25];

  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (const oy of sub) {
        for (const ox of sub) {
          const x = ((px + 0.5 + ox) / size) * 2 - 1;
          const y = ((py + 0.5 + oy) / size) * 2 - 1;
          const s = sampleMascot(x, y, pxSize);
          r += s[0] * s[3];
          g += s[1] * s[3];
          b += s[2] * s[3];
          a += s[3];
        }
      }
      const idx = (py * size + px) * 4;
      const outA = a / 4;
      if (outA > 0) {
        buf[idx] = clamp(Math.round((r / a) * 255), 0, 255);
        buf[idx + 1] = clamp(Math.round((g / a) * 255), 0, 255);
        buf[idx + 2] = clamp(Math.round((b / a) * 255), 0, 255);
        buf[idx + 3] = clamp(Math.round(outA * 255), 0, 255);
      }
    }
  }
  return buf;
}

// ---- Minimal Pure-JS PNG Encoder ----
const CRC_TABLE = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
  CRC_TABLE[n] = c >>> 0;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function pngChunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii');
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}
function encodePNG(width, height, rgba) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;

  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0; // filter: None
    raw.set(rgba.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', idat),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---- Multi-Size ICO Encoder (PNG-in-ICO, Windows Vista/10/11 & Tauri compatible) ----
function encodeICO(entries) {
  // entries: [{ size, pngBuf }]
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type = ICO
  header.writeUInt16LE(entries.length, 4);

  const dir = Buffer.alloc(entries.length * 16);
  let offset = 6 + dir.length;

  entries.forEach((e, i) => {
    const base = i * 16;
    dir[base] = e.size >= 256 ? 0 : e.size;
    dir[base + 1] = e.size >= 256 ? 0 : e.size;
    dir[base + 2] = 0; // palette
    dir[base + 3] = 0; // reserved
    dir.writeUInt16LE(1, base + 4); // planes
    dir.writeUInt16LE(32, base + 6); // bpp
    dir.writeUInt32LE(e.pngBuf.length, base + 8);
    dir.writeUInt32LE(offset, base + 12);
    offset += e.pngBuf.length;
  });

  return Buffer.concat([header, dir, ...entries.map((e) => e.pngBuf)]);
}

// ---- Generate All Icon Artifacts ----
const assetsDir = path.join(ROOT, 'assets');
const iconsDir = path.join(ROOT, 'src-tauri', 'icons');
fs.mkdirSync(assetsDir, { recursive: true });
fs.mkdirSync(iconsDir, { recursive: true });

const sizes = [16, 24, 32, 48, 64, 128, 256, 512];
const pngBySize = new Map();

for (const s of sizes) {
  const rgba = renderRGBA(s);
  pngBySize.set(s, encodePNG(s, s, rgba));
}

fs.writeFileSync(path.join(assetsDir, 'pet_icon.png'), pngBySize.get(512));
fs.writeFileSync(path.join(iconsDir, 'icon.png'), pngBySize.get(512));
fs.writeFileSync(path.join(iconsDir, '128x128@2x.png'), pngBySize.get(256));
fs.writeFileSync(path.join(iconsDir, '128x128.png'), pngBySize.get(128));
fs.writeFileSync(path.join(iconsDir, '32x32.png'), pngBySize.get(32));

const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const icoBuf = encodeICO(icoSizes.map((s) => ({ size: s, pngBuf: pngBySize.get(s) })));
fs.writeFileSync(path.join(iconsDir, 'icon.ico'), icoBuf);

console.log('[gen_pet_icon] Generated cute 2D chibi pet mascot icons:');
for (const rel of [
  'assets/pet_icon.png',
  'src-tauri/icons/icon.png',
  'src-tauri/icons/128x128@2x.png',
  'src-tauri/icons/128x128.png',
  'src-tauri/icons/32x32.png',
  'src-tauri/icons/icon.ico',
]) {
  const full = path.join(ROOT, rel);
  const st = fs.statSync(full);
  console.log(`  - ${rel} (${st.size} bytes)`);
}
