'use strict';
const fs = require('fs');
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const paths = require('./paths');

function pipePath(home = paths.PET_HOME) {
  if (process.platform !== 'win32') return path.join(home, 'pet.sock');
  return '\\\\.\\pipe\\nibble-' + crypto.createHash('sha1').update(home.toLowerCase()).digest('hex').slice(0, 12);
}

function install() {
  fs.mkdirSync(paths.PET_HOME, { recursive: true });
  fs.copyFileSync(paths.CLI_SOURCE, paths.CLI_FILE);
}

function writeAppInfo(info) {
  fs.mkdirSync(paths.PET_HOME, { recursive: true });
  fs.writeFileSync(paths.APP_INFO_FILE, JSON.stringify(info, null, 2));
}

function listen(onCommand) {
  const file = pipePath();
  if (process.platform !== 'win32') fs.rmSync(file, { force: true });
  const server = net.createServer((socket) => {
    socket.setEncoding('utf8');
    let text = '';
    let taken = false;
    socket.on('error', () => {});
    socket.on('data', (chunk) => {
      if (taken) return;
      text += chunk;
      const end = text.indexOf('\n');
      if (end < 0) { if (text.length > 20000) socket.destroy(); return; }
      taken = true;
      const line = text.slice(0, end).trim();
      const space = line.indexOf(' ');
      const word = space < 0 ? line : line.slice(0, space);
      let payload = null;
      if (space > 0) { try { payload = JSON.parse(line.slice(space + 1)); } catch {} }
      const connection = { onClose: (fn) => socket.on('close', fn) };
      Promise.resolve()
        .then(() => onCommand(word, payload, connection))
        .catch(() => null)
        .then((reply) => { if (!socket.destroyed) socket.end(JSON.stringify(reply || { ok: false }) + '\n'); });
    });
  });
  server.on('error', () => {});
  server.listen(file);
  return () => server.close();
}

module.exports = { pipePath, install, writeAppInfo, listen };
