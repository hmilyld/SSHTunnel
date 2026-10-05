/**
 * 临时审计脚本（用完即删）：解码 icons/ 下所有图标，报告真实颜色内容。
 *   node scripts/_icon_audit.mjs
 * 零依赖：PNG 手动 inflate + 反滤波。
 */
import { inflateSync } from "node:zlib";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
/** icns 内嵌 PNG 导出目录，方便肉眼检查（在 gitignore 的 dist/ 里） */
const previewDir = join(dirname(fileURLToPath(import.meta.url)), "..", "dist", "icon-preview");
mkdirSync(previewDir, { recursive: true });

function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error("不是 PNG");
  let off = 8;
  let ihdr = null;
  const idat = [];
  while (off + 8 <= buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString("ascii", off + 4, off + 8);
    const data = buf.subarray(off + 8, off + 8 + len);
    if (type === "IHDR") {
      ihdr = {
        width: data.readUInt32BE(0),
        height: data.readUInt32BE(4),
        bitDepth: data[8],
        colorType: data[9],
        interlace: data[12],
      };
    } else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
    off += 12 + len;
  }
  if (!ihdr) throw new Error("缺少 IHDR");
  const { width, height, bitDepth, colorType, interlace } = ihdr;
  if (bitDepth !== 8 || interlace !== 0) return { ...ihdr, note: `未解码(bitDepth=${bitDepth},interlace=${interlace})` };
  const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colorType];
  if (!channels) return { ...ihdr, note: `未解码(colorType=${colorType})` };
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(height * stride);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    const cur = Buffer.from(line);
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? cur[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      if (filter === 1) cur[i] = (cur[i] + a) & 0xff;
      else if (filter === 2) cur[i] = (cur[i] + b) & 0xff;
      else if (filter === 3) cur[i] = (cur[i] + ((a + b) >> 1)) & 0xff;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a),
          pb = Math.abs(p - b),
          pc = Math.abs(p - c);
        cur[i] = (cur[i] + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c)) & 0xff;
      }
    }
    cur.copy(out, y * stride);
    prev = cur;
  }
  return { ...ihdr, channels, pixels: out };
}

function report(name, buf) {
  let img;
  try {
    img = decodePng(buf);
  } catch (e) {
    console.log(`    ${name}: 解析失败 ${e.message}`);
    return;
  }
  if (!img.pixels) {
    console.log(`    ${name}: ${img.width}x${img.height} ${img.note} 字节=${buf.length}`);
    return;
  }
  const { width, height, channels, pixels } = img;
  const step = Math.max(1, Math.floor((width * height) / 20000));
  let n = 0,
    spreadSum = 0,
    maxSpread = 0,
    colorful = 0;
  let minX = width,
    minY = height,
    maxX = -1,
    maxY = -1;
  const samples = [];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = y * width + x;
      if (i % step !== 0) {
        // 仍然统计 bbox（全像素扫描，便宜），只是颜色统计按 step 抽样
      }
      const o = i * channels;
      const a = channels === 4 ? pixels[o + 3] : channels === 2 ? pixels[o + 1] : 255;
      if (a <= 16) continue;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
      if (i % step !== 0) continue;
      const r = pixels[o],
        g = channels >= 3 ? pixels[o + 1] : r,
        b = channels >= 3 ? pixels[o + 2] : r;
      const spread = Math.max(r, g, b) - Math.min(r, g, b);
      n++;
      spreadSum += spread;
      if (spread > maxSpread) maxSpread = spread;
      if (spread > 40) colorful++;
      if (samples.length < 3 && n % 7 === 1) samples.push(`rgb(${r},${g},${b})`);
    }
  }
  const artW = maxX - minX + 1;
  const marginPct = (minX / width) * 100;
  console.log(
    `    ${name}: ${width}x${height} ch=${channels} 不透明区=${artW}x${maxY - minY + 1} ` +
      `左边距=${marginPct.toFixed(2)}% (期望满画布 0.00% / mac 栅格 9.77%) 平均色差=${(spreadSum / Math.max(1, n)).toFixed(1)} ` +
      `彩色占比=${((colorful / Math.max(1, n)) * 100).toFixed(1)}% 例=${samples.join(" ")}`,
  );
}

console.log("== 独立 PNG ==");
for (const f of ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png"]) {
  report(f, readFileSync(join(dir, f)));
}

console.log("== icon.icns ==");
{
  const buf = readFileSync(join(dir, "icon.icns"));
  const declared = buf.readUInt32BE(4);
  console.log(`  magic=${buf.toString("ascii", 0, 4)} 声明长度=${declared} 实际=${buf.length}`);
  let off = 8;
  while (off + 8 <= buf.length) {
    const fourcc = buf.toString("ascii", off, off + 4);
    const len = buf.readUInt32BE(off + 4);
    const payload = buf.subarray(off + 8, off + len);
    const isPng = payload.readUInt32BE(0) === 0x89504e47;
    console.log(`  [${fourcc}] len=${len} payload=${payload.length} ${isPng ? "PNG" : "非PNG:" + payload.toString("hex", 0, 4)}`);
    if (isPng) {
      report(fourcc, payload);
      writeFileSync(join(previewDir, `${fourcc}.png`), payload);
    }
    off += len;
  }
}

console.log("== icon.ico ==");
{
  const buf = readFileSync(join(dir, "icon.ico"));
  const count = buf.readUInt16LE(4);
  console.log(`  type=${buf.readUInt16LE(2)} count=${count} 实际=${buf.length}`);
  for (let i = 0; i < count; i++) {
    const e = 6 + i * 16;
    const w = buf[e] || 256;
    const h = buf[e + 1] || 256;
    const size = buf.readUInt32LE(e + 8);
    const offset = buf.readUInt32LE(e + 12);
    const payload = buf.subarray(offset, offset + size);
    const isPng = payload.readUInt32BE(0) === 0x89504e47;
    console.log(`  entry${i}: ${w}x${h} bytes=${size} ${isPng ? "PNG" : "非PNG(bmp)"}`);
    if (isPng) report(`ico-${w}`, payload);
  }
}
