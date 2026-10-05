/**
 * 应用图标生成脚本（零依赖）：
 *   node scripts/gen-icons.mjs
 *
 * 生成 Tauri 打包与托盘所需的全部图标：
 *   icons/32x32.png  icons/128x128.png  icons/128x128@2x.png
 *   icons/icon.png(1024)  icons/icon.ico  icons/icon.icns
 *
 * 图形：蓝→靛蓝渐变圆角方块 + 白色右向箭头（端口转发语义）。
 * 采用 4x4 超采样抗锯齿，按尺寸直接程序化绘制，无缩放失真。
 *
 * 平台差异（重要）：**.icns 按 Apple 的 macOS 图标栅格渲染**（圆角方块 824×824 居中放在
 * 1024 画布上，四周 100px 透明边距，圆角半径 185.4）；Windows/Linux 的 PNG/ICO 仍按
 * 满画布渲染。原因：macOS 26 (Tahoe) 会把不符合该栅格的图标缩小后套进灰色圆角底框
 * （社区叫 "icon jail"，看起来就是一团灰白方块），而满画布的图标在 Dock 里也一向比
 * 系统图标更大。详见 README「已知边界」。
 */
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = join(__dirname, "..", "src-tauri", "icons");

/* ---------------- PNG 编码 ---------------- */

const crcTable = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

function encodePng(size, rgba) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  // 每行前加 filter type 0
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    const rowStart = y * (size * 4 + 1);
    raw[rowStart] = 0;
    Buffer.from(rgba.buffer, rgba.byteOffset + y * size * 4, size * 4).copy(raw, rowStart + 1);
  }
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([sig, pngChunk("IHDR", ihdr), pngChunk("IDAT", idat), pngChunk("IEND", Buffer.alloc(0))]);
}

/* ---------------- 图形绘制 ---------------- */

const C1 = [14, 165, 233]; // #0EA5E9 sky-500
const C2 = [79, 70, 229]; // #4F46E5 indigo-600

/**
 * Apple HIG 的 macOS 图标栅格：1024 画布里放 824×824 的圆角方块并居中（四周 100px 边距），
 * 圆角半径 185.4（= 22.5% × 824）。只有 .icns 用它。
 */
const MAC_GRID = { canvas: 1024, art: 824, radius: 185.4 };
/** 非 macOS 目标：满画布圆角方块，圆角半径占边长 22% */
const FULL_BLEED_RADIUS_RATIO = 0.22;

function lerp(a, b, t) {
  return Math.round(a + (b - a) * t);
}

/** 点是否在“圆角矩形”内 */
function inRoundedRect(x, y, s, r) {
  const nx = Math.min(Math.max(x, r), s - r);
  const ny = Math.min(Math.max(y, r), s - r);
  const dx = x - nx;
  const dy = y - ny;
  return dx * dx + dy * dy <= r * r;
}

/** 点是否在三角形内（重心法） */
function inTriangle(px, py, ax, ay, bx, by, cx, cy) {
  const d1 = (px - bx) * (ay - by) - (ax - bx) * (py - by);
  const d2 = (px - cx) * (by - cy) - (bx - cx) * (py - cy);
  const d3 = (px - ax) * (cy - ay) - (cx - ax) * (py - ay);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

/** 归一化坐标下的箭头（右向）：横杆 + 三角箭头 */
function inArrow(u, v) {
  const shaft = u >= 0.22 && u <= 0.66 && v >= 0.445 && v <= 0.555;
  const head = inTriangle(u, v, 0.6, 0.3, 0.6, 0.7, 0.84, 0.5);
  return shaft || head;
}

/**
 * 4x4 超采样渲染一个尺寸的 RGBA 图。
 *
 * macGrid=true 时圆角方块缩到画布的 824/1024 并居中，箭头与渐变都按方块内部归一化坐标绘制，
 * 因此在方块里的相对大小、位置与非 macOS 版本完全一致（.icns 专用）。
 */
function renderIcon(size, { macGrid = false } = {}) {
  const rgba = Buffer.alloc(size * size * 4);
  const SS = 4;
  const art = macGrid ? (size * MAC_GRID.art) / MAC_GRID.canvas : size;
  const offset = (size - art) / 2;
  const r = (macGrid ? MAC_GRID.radius / MAC_GRID.art : FULL_BLEED_RADIUS_RATIO) * art;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let accA = 0,
        accR = 0,
        accG = 0,
        accB = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          // 方块内部坐标（非 macGrid 时 offset=0、art=size，与旧实现一致）
          const px = x + (sx + 0.5) / SS - offset;
          const py = y + (sy + 0.5) / SS - offset;
          if (!inRoundedRect(px, py, art, r)) continue;
          const t = (px / art + py / art) / 2;
          const inside = inArrow(px / art, py / art);
          const cr = inside ? 255 : lerp(C1[0], C2[0], t);
          const cg = inside ? 255 : lerp(C1[1], C2[1], t);
          const cb = inside ? 255 : lerp(C1[2], C2[2], t);
          accA += 255;
          accR += cr;
          accG += cg;
          accB += cb;
        }
      }
      const n = SS * SS;
      const i = (y * size + x) * 4;
      rgba[i] = Math.round(accR / n);
      rgba[i + 1] = Math.round(accG / n);
      rgba[i + 2] = Math.round(accB / n);
      rgba[i + 3] = Math.round(accA / n);
    }
  }
  return rgba;
}

/* ---------------- ICO / ICNS 封装 ---------------- */

function buildIco(entries /* [{ size, png }] */) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type = icon
  header.writeUInt16LE(entries.length, 4);
  const dirs = [];
  const blobs = [];
  let offset = 6 + entries.length * 16;
  for (const { size, png } of entries) {
    const dir = Buffer.alloc(16);
    dir[0] = size >= 256 ? 0 : size; // width (0 = 256)
    dir[1] = size >= 256 ? 0 : size; // height
    dir[2] = 0; // palette
    dir[3] = 0; // reserved
    dir.writeUInt16LE(1, 4); // planes
    dir.writeUInt16LE(32, 6); // bpp
    dir.writeUInt32LE(png.length, 8);
    dir.writeUInt32LE(offset, 12);
    dirs.push(dir);
    blobs.push(png);
    offset += png.length;
  }
  return Buffer.concat([header, ...dirs, ...blobs]);
}

function buildIcns(entries /* [{ fourcc, png }] */) {
  const body = [];
  for (const { fourcc, png } of entries) {
    const head = Buffer.alloc(8);
    head.write(fourcc, 0, "ascii");
    head.writeUInt32BE(8 + png.length, 4);
    body.push(head, png);
  }
  const total = 8 + body.reduce((n, b) => n + b.length, 0);
  const magic = Buffer.alloc(8);
  magic.write("icns", 0, "ascii");
  magic.writeUInt32BE(total, 4);
  return Buffer.concat([magic, ...body]);
}

/* ---------------- 主流程 ---------------- */

mkdirSync(outDir, { recursive: true });

const pngOf = new Map();
const png = (size) => {
  if (!pngOf.has(size)) pngOf.set(size, encodePng(size, renderIcon(size)));
  return pngOf.get(size);
};

/** macOS 栅格版本（仅 .icns 使用）：方块缩到 824/1024，四周留透明边距 */
const macPngOf = new Map();
const macPng = (size) => {
  if (!macPngOf.has(size)) macPngOf.set(size, encodePng(size, renderIcon(size, { macGrid: true })));
  return macPngOf.get(size);
};

// 标准 PNG 图标
writeFileSync(join(outDir, "32x32.png"), png(32));
writeFileSync(join(outDir, "128x128.png"), png(128));
writeFileSync(join(outDir, "128x128@2x.png"), png(256));
writeFileSync(join(outDir, "icon.png"), png(1024));

// Windows ICO（PNG 压缩条目：16/32/48/256）
writeFileSync(
  join(outDir, "icon.ico"),
  buildIco([
    { size: 16, png: png(16) },
    { size: 32, png: png(32) },
    { size: 48, png: png(48) },
    { size: 256, png: png(256) },
  ]),
);

// macOS ICNS（PNG 容器条目）：按 Apple 的 824-on-1024 图标栅格渲染
writeFileSync(
  join(outDir, "icon.icns"),
  buildIcns([
    { fourcc: "ic11", png: macPng(32) }, // 16@2x
    { fourcc: "ic12", png: macPng(64) }, // 32@2x
    { fourcc: "ic07", png: macPng(128) },
    { fourcc: "ic08", png: macPng(256) },
    { fourcc: "ic09", png: macPng(512) },
    { fourcc: "ic10", png: macPng(1024) },
  ]),
);

console.log("icons generated ->", outDir);
