'use strict';
const { execFile } = require('child_process');

function pidAlive(pid) {
  if (!pid) return false;
  try { process.kill(pid, 0); return true; } catch (err) { return err.code === 'EPERM'; }
}

function listClaudePids() {
  return new Promise((resolve) => {
    if (process.platform === 'win32') {
      execFile('tasklist', ['/FI', 'IMAGENAME eq claude.exe', '/FO', 'CSV', '/NH'], { windowsHide: true }, (err, out) => {
        if (err) return resolve(null);
        resolve(String(out).split(/\r?\n/).map((line) => /^"claude\.exe","(\d+)"/i.exec(line)).filter(Boolean).map((m) => Number(m[1])));
      });
    } else {
      execFile('ps', ['-A', '-o', 'pid=,comm='], (err, out) => {
        if (err) return resolve(null);
        resolve(String(out).split('\n').map((line) => /^\s*(\d+)\s+(.*)$/.exec(line)).filter((m) => m && /(^|\/)claude$/i.test(m[2].trim())).map((m) => Number(m[1])));
      });
    }
  });
}

function watchClaude({ sessionPids, onGone, every = 5000, misses = 3, list = listClaudePids, alive = pidAlive }) {
  let known = [];
  let seen = false;
  let missed = 0;
  let busy = false;

  const check = async () => {
    if (busy) return;
    busy = true;
    try {
      known = known.filter(alive);
      let present = known.length > 0 || sessionPids().some(alive);
      if (!present) {
        const found = await list();
        if (found === null) return;
        known = found.filter(alive);
        present = known.length > 0;
      }
      if (present) { seen = true; missed = 0; return; }
      if (seen && ++missed >= misses) { stop(); onGone(); }
    } finally {
      busy = false;
    }
  };
  const timer = setInterval(check, every);
  const stop = () => clearInterval(timer);
  check();
  return stop;
}

module.exports = { watchClaude, listClaudePids, pidAlive };
