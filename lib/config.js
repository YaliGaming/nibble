'use strict';
const fs = require('fs');
const path = require('path');
const { CONFIG_FILE } = require('./paths');

const DEFAULTS = {
  pet: 'ember',
  size: 1,
  anchor: null,
  visible: true,
  lang: 'auto',
  shortcut: 'Control+Alt+P',
  quickMode: 'background',
  quickCwd: '',
  welcomed: false,
  showBackground: false,
  storeText: true,
  sound: false,
  notify: false,
  approvals: true,
};

function load() {
  try {
    const saved = JSON.parse(fs.readFileSync(CONFIG_FILE, 'utf8'));
    return { ...DEFAULTS, ...saved };
  } catch {
    return { ...DEFAULTS };
  }
}

function save(config) {
  try {
    fs.mkdirSync(path.dirname(CONFIG_FILE), { recursive: true });
    const tmp = CONFIG_FILE + '.tmp';
    fs.writeFileSync(tmp, JSON.stringify(config, null, 2));
    fs.renameSync(tmp, CONFIG_FILE);
  } catch {}
}

module.exports = { DEFAULTS, load, save };
