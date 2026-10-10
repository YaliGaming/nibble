'use strict';
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const https = require('https');
const petsLib = require('../lib/pets');

const root = path.join(__dirname, '..');
const out = path.join(root, 'build');

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xFFFFFFFF;
  for (const byte of buf) c = crcTable[(c ^ byte) & 0xFF] ^ (c >>> 8);
  return (c ^ 0xFFFFFFFF) >>> 0;
}

function png(bgra, size) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    const row = y * (size * 4 + 1);
    for (let x = 0; x < size; x++) {
      const i = (y * size + x) * 4;
      raw[row + 1 + x * 4] = bgra[i + 2];
      raw[row + 2 + x * 4] = bgra[i + 1];
      raw[row + 3 + x * 4] = bgra[i];
      raw[row + 4 + x * 4] = bgra[i + 3];
    }
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const head = Buffer.alloc(4); head.writeUInt32BE(data.length);
    const tail = Buffer.alloc(4); tail.writeUInt32BE(crc32(body));
    return Buffer.concat([head, body, tail]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0); header.writeUInt32BE(size, 4);
  header[8] = 8; header[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
    chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0)),
  ]);
}

function dib(bgra, size) {
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0); header.writeInt32LE(size, 4); header.writeInt32LE(size * 2, 8);
  header.writeUInt16LE(1, 12); header.writeUInt16LE(32, 14);
  const pixels = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) bgra.copy(pixels, (size - 1 - y) * size * 4, y * size * 4, (y + 1) * size * 4);
  const mask = Buffer.alloc(Math.ceil(size / 32) * 4 * size);
  return Buffer.concat([header, pixels, mask]);
}

function ico(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2); header.writeUInt16LE(images.length, 4);
  let offset = 6 + 16 * images.length;
  const entries = images.map(({ size, data }) => {
    const e = Buffer.alloc(16);
    e[0] = size === 256 ? 0 : size; e[1] = size === 256 ? 0 : size;
    e.writeUInt16LE(1, 4); e.writeUInt16LE(32, 6);
    e.writeUInt32LE(data.length, 8); e.writeUInt32LE(offset, 12);
    offset += data.length;
    return e;
  });
  return Buffer.concat([header, ...entries, ...images.map((i) => i.data)]);
}

function makeIcon() {
  const pet = petsLib.loadAll().find((p) => p.id === 'ember');
  const draw = (size) => petsLib.trayBitmap(pet, size);
  fs.mkdirSync(out, { recursive: true });
  fs.writeFileSync(path.join(out, 'icon.ico'), ico([16, 32, 48, 64, 128, 256].map((size) => ({
    size, data: size === 256 ? png(draw(size), size) : dib(draw(size), size),
  }))));
  fs.writeFileSync(path.join(out, 'icon.png'), png(draw(256), 256));
  console.log('build/icon.ico and build/icon.png drawn from', pet.name);
}

function download(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode !== 200) { res.resume(); reject(new Error(`${url} answered ${res.statusCode}`)); return; }
      const parts = [];
      res.on('data', (c) => parts.push(c));
      res.on('end', () => resolve(Buffer.concat(parts)));
    }).on('error', reject);
  });
}

async function bundleNode() {
  const dir = path.join(out, 'node');
  fs.mkdirSync(dir, { recursive: true });
  const name = process.platform === 'win32' ? 'node.exe' : 'node';
  fs.copyFileSync(process.execPath, path.join(dir, name));
  const beside = path.join(path.dirname(process.execPath), 'LICENSE');
  if (fs.existsSync(beside)) fs.copyFileSync(beside, path.join(dir, 'LICENSE'));
  else fs.writeFileSync(path.join(dir, 'LICENSE'), await download(`https://raw.githubusercontent.com/nodejs/node/v${process.versions.node}/LICENSE`));
  console.log(`build/node: Node.js ${process.versions.node} (${process.arch}) and its licence`);
}

(async () => {
  if (process.argv.includes('--icon') || !fs.existsSync(path.join(out, 'icon.ico'))) makeIcon();
  if (!process.argv.includes('--icon')) await bundleNode();
})().catch((err) => { console.error(String(err.message || err)); process.exit(1); });
