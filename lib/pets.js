'use strict';
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const sprites = require('../ui/sprites.js');
const paths = require('./paths');

function readDir(dir, custom) {
  let names = [];
  try { names = fs.readdirSync(dir).filter((n) => n.endsWith('.json')).sort(); } catch { return []; }
  const out = [];
  for (const name of names) {
    try {
      const pet = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
      if (!sprites.validatePet(pet).ok) continue;
      const id = (custom ? 'custom-' : '') + name.slice(0, -5).replace(/[^A-Za-z0-9_-]/g, '');
      out.push({ ...pet, id, custom, name: String(pet.name).slice(0, 40) });
    } catch {}
  }
  return out;
}

function loadAll() {
  return [...readDir(paths.BUILTIN_PETS_DIR, false), ...readDir(paths.CUSTOM_PETS_DIR, true)];
}

function portable(pet) {
  const out = { format: 1, name: pet.name, palette: pet.palette, base: pet.base, frames: pet.frames || {} };
  if (pet.outline !== undefined) out.outline = pet.outline;
  return out;
}

function saveCustom(pet) {
  const check = sprites.validatePet(pet);
  if (!check.ok) throw new Error(check.errors.slice(0, 3).join('; '));
  fs.mkdirSync(paths.CUSTOM_PETS_DIR, { recursive: true });
  const slug = String(pet.name).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 24) || 'pet';
  const file = `${slug}-${crypto.randomBytes(2).toString('hex')}`;
  const clean = portable(pet);
  fs.writeFileSync(path.join(paths.CUSTOM_PETS_DIR, file + '.json'), JSON.stringify(clean, null, 2) + '\n');
  return 'custom-' + file;
}

function deleteCustom(id) {
  if (!id.startsWith('custom-')) return;
  const file = id.slice('custom-'.length).replace(/[^A-Za-z0-9_-]/g, '');
  if (file) fs.rmSync(path.join(paths.CUSTOM_PETS_DIR, file + '.json'), { force: true });
}

function formatSpec() {
  const text = fs.readFileSync(paths.SKILL_SOURCE, 'utf8');
  const start = text.indexOf('## Pet format');
  const end = text.indexOf('## Saving the pet');
  return text.slice(start, end > start ? end : undefined).trim();
}

function hatchPrompt(description) {
  return [
    'Design a pixel-art desktop pet for the "Nibble" app.',
    '',
    formatSpec(),
    '',
    'Reply with ONLY the pet JSON object: no prose, no code fence. Do not use any tools.',
    '',
    'The pet to design: ' + description,
  ].join('\n');
}

function extractJson(text) {
  const start = text.indexOf('{');
  if (start < 0) throw new Error('No JSON found in the reply');
  let depth = 0;
  let inString = false;
  for (let i = start; i < text.length; i++) {
    const ch = text[i];
    if (inString) {
      if (ch === '\\') i++;
      else if (ch === '"') inString = false;
    } else if (ch === '"') inString = true;
    else if (ch === '{') depth++;
    else if (ch === '}' && --depth === 0) return JSON.parse(text.slice(start, i + 1));
  }
  throw new Error('The pet JSON was cut off');
}

function trayBitmap(pet, size = 32) {
  const rows = sprites.frameRows(pet, 'base');
  const w = rows[0].length;
  const h = rows.length;
  const scale = Math.max(1, Math.floor(size / Math.max(w, h)));
  const ox = Math.floor((size - w * scale) / 2);
  const oy = Math.floor((size - h * scale) / 2);
  const buf = Buffer.alloc(size * size * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const ch = rows[y][x];
      if (ch === '.' || ch === ' ') continue;
      const [r, g, b] = sprites.hexToRgb(pet.palette[ch]);
      for (let sy = 0; sy < scale; sy++) {
        for (let sx = 0; sx < scale; sx++) {
          const px = ox + x * scale + sx;
          const py = oy + y * scale + sy;
          if (px < 0 || py < 0 || px >= size || py >= size) continue;
          const i = (py * size + px) * 4;
          buf[i] = b; buf[i + 1] = g; buf[i + 2] = r; buf[i + 3] = 255;
        }
      }
    }
  }
  return buf;
}

module.exports = { loadAll, saveCustom, deleteCustom, portable, hatchPrompt, extractJson, trayBitmap };
