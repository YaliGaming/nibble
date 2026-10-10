'use strict';
const fs = require('fs');
const path = require('path');
const paths = require('./paths');

function loadAll(dir = paths.LOCALES_DIR) {
  const tables = {};
  let names = [];
  try { names = fs.readdirSync(dir).filter((n) => /^[a-z]{2,3}(-[A-Za-z]{2,4})?\.json$/.test(n)).sort(); } catch {}
  for (const name of names) {
    try {
      const table = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
      if (table && typeof table === 'object' && !Array.isArray(table)) tables[name.slice(0, -5).toLowerCase()] = table;
    } catch {}
  }
  if (!tables.en) tables.en = { $name: 'English', $dir: 'ltr' };
  return tables;
}

function pick(tables, setting, preferred) {
  const has = (code) => Object.prototype.hasOwnProperty.call(tables, code);
  if (typeof setting === 'string' && has(setting.toLowerCase())) return setting.toLowerCase();
  for (const tag of preferred || []) {
    const full = String(tag).toLowerCase();
    if (has(full)) return full;
    const base = full.split(/[-_]/)[0];
    if (has(base)) return base;
  }
  return 'en';
}

function strings(tables, code) {
  return { ...tables.en, ...(tables[code] || {}) };
}

const list = (tables) => Object.keys(tables).map((code) => ({ code, name: tables[code].$name || code }));

module.exports = { loadAll, pick, strings, list };
