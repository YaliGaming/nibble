'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');

const PET_HOME = process.env.NIBBLE_HOME || path.join(os.homedir(), '.nibble-pet');
const CLAUDE_HOME = process.env.CLAUDE_CONFIG_DIR || path.join(os.homedir(), '.claude');

function realDir(dir) {
  try { return fs.realpathSync.native(dir); } catch { return dir; }
}
const APP_DIR = realDir(path.join(__dirname, '..'));

function bundledNode() {
  const file = path.join(APP_DIR, '..', 'node', process.platform === 'win32' ? 'node.exe' : 'node');
  return fs.existsSync(file) ? realDir(file) : 'node';
}

module.exports = {
  PET_HOME,
  CLAUDE_HOME,
  APP_DIR,
  EVENTS_DIR: path.join(PET_HOME, 'events'),
  LOCALES_DIR: path.join(APP_DIR, 'locales'),
  NODE_BIN: bundledNode(),
  CUSTOM_PETS_DIR: path.join(PET_HOME, 'pets'),
  BUILTIN_PETS_DIR: path.join(APP_DIR, 'pets'),
  QUICK_DIR: path.join(PET_HOME, 'quick-chat'),
  TMP_DIR: path.join(PET_HOME, 'tmp'),
  BACKUP_DIR: path.join(PET_HOME, 'backups'),
  CONFIG_FILE: path.join(PET_HOME, 'config.json'),
  HOOK_FILE: path.join(PET_HOME, 'hook.js'),
  HOOK_SOURCE: path.join(APP_DIR, 'hook', 'pet-hook.js'),
  SKILL_SOURCE: path.join(APP_DIR, 'skill', 'hatch-pet', 'SKILL.md'),
  SKILL_DIR: path.join(CLAUDE_HOME, 'skills', 'hatch-pet'),
  CLI_FILE: path.join(PET_HOME, 'pet.js'),
  CLI_SOURCE: path.join(APP_DIR, 'cli', 'pet.js'),
  PET_SKILL_SOURCE: path.join(APP_DIR, 'skill', 'pet', 'SKILL.md'),
  PET_SKILL_DIR: path.join(CLAUDE_HOME, 'skills', 'pet'),
  APP_INFO_FILE: path.join(PET_HOME, 'app.json'),
  SETTINGS_FILE: path.join(CLAUDE_HOME, 'settings.json'),
  REGISTRY_DIR: path.join(CLAUDE_HOME, 'sessions'),
};
