'use strict';
const fs = require('fs');
const path = require('path');
const { EventEmitter } = require('events');
const reducer = require('./reducer');

const DAY = 86400e3;
const MAX_EVENTS = 2000;
const sanitize = (sid) => String(sid || '').replace(/[^A-Za-z0-9_-]/g, '');

function pidAlive(pid) {
  if (!pid) return false;
  try { process.kill(pid, 0); return true; } catch (err) { return err.code === 'EPERM'; }
}

class SessionStore extends EventEmitter {
  constructor({ eventsDir, registryDir, readyTtlMin = 30 }) {
    super();
    this.eventsDir = eventsDir;
    this.registryDir = registryDir;
    this.readyTtlMin = readyTtlMin;
    this.records = new Map();
    this.offsets = new Map();
    this.timers = [];
    this.watcher = null;
    this.dirty = false;
  }

  start() {
    fs.mkdirSync(this.eventsDir, { recursive: true });
    this.prune();
    this.scan();
    this.pollRegistry(true);
    try {
      this.watcher = fs.watch(this.eventsDir, (_type, name) => {
        if (name && name.endsWith('.jsonl')) { this.readLog(name); this.flush(); }
      });
      this.watcher.on('error', () => {});
    } catch {}
    this.timers.push(setInterval(() => { this.pollRegistry(false); this.flush(); }, 1000));
    this.timers.push(setInterval(() => { this.scan(); this.flush(); }, 2500));
    this.timers.push(setInterval(() => this.emit('change'), 30e3));
    this.flush();
  }

  stop() {
    this.timers.forEach(clearInterval);
    this.timers = [];
    if (this.watcher) { this.watcher.close(); this.watcher = null; }
  }

  flush() {
    if (!this.dirty) return;
    this.dirty = false;
    this.emit('change');
  }

  prune() {
    let names = [];
    try { names = fs.readdirSync(this.eventsDir); } catch { return; }
    const cutoff = Date.now() - 7 * DAY;
    for (const name of names) {
      if (!name.endsWith('.jsonl')) continue;
      const file = path.join(this.eventsDir, name);
      try { if (fs.statSync(file).mtimeMs < cutoff) fs.unlinkSync(file); } catch {}
    }
  }

  scan() {
    let names = [];
    try { names = fs.readdirSync(this.eventsDir); } catch { return; }
    for (const name of names) if (name.endsWith('.jsonl')) this.readLog(name);
  }

  readLog(name) {
    const file = path.join(this.eventsDir, name);
    const sid = name.slice(0, -'.jsonl'.length);
    let size;
    try { size = fs.statSync(file).size; } catch { return; }
    let offset = this.offsets.get(name) || 0;
    if (size < offset) { offset = 0; this.records.delete(sid); }
    if (size === offset) return;

    const buf = Buffer.alloc(size - offset);
    let fd;
    try {
      fd = fs.openSync(file, 'r');
      fs.readSync(fd, buf, 0, buf.length, offset);
    } catch { return; } finally { if (fd !== undefined) fs.closeSync(fd); }

    const end = buf.lastIndexOf(0x0a);
    if (end < 0) return;
    this.offsets.set(name, offset + end + 1);

    const events = [];
    for (const line of buf.subarray(0, end).toString('utf8').split('\n')) {
      if (!line.trim()) continue;
      try {
        const e = JSON.parse(line);
        if (e && e.ev) events.push(e);
      } catch {}
    }
    if (events.length) this.ingest(sid, events);
  }

  ingest(sid, events) {
    let rec = this.records.get(sid);
    if (!rec) {
      rec = { base: reducer.blank(sid), events: [], state: reducer.blank(sid), lastT: 0 };
      this.records.set(sid, rec);
    }
    let outOfOrder = false;
    for (const e of events) {
      if ((e.t || 0) < rec.lastT) outOfOrder = true;
      else rec.lastT = e.t || 0;
      rec.events.push(e);
    }
    if (outOfOrder) {
      rec.events.sort((a, b) => (a.t || 0) - (b.t || 0));
      rec.state = structuredClone(rec.base);
      for (const e of rec.events) reducer.apply(rec.state, e);
    } else {
      for (const e of events) reducer.apply(rec.state, e);
    }
    if (rec.events.length > MAX_EVENTS) {
      for (const e of rec.events.splice(0, MAX_EVENTS / 2)) reducer.apply(rec.base, e);
    }
    this.dirty = true;
  }

  append(sid, event) {
    sid = sanitize(sid);
    if (!sid) return;
    const e = { t: Date.now(), ...event, sid };
    try {
      fs.mkdirSync(this.eventsDir, { recursive: true });
      fs.appendFileSync(path.join(this.eventsDir, sid + '.jsonl'), JSON.stringify(e) + '\n');
    } catch { return; }
    this.readLog(sid + '.jsonl');
  }

  record(sid, event) {
    this.append(sid, event);
    this.flush();
  }

  pollRegistry(first) {
    let names;
    try { names = fs.readdirSync(this.registryDir); } catch { names = []; }
    const seen = new Set();
    const now = Date.now();
    for (const name of names) {
      if (!/^\d+\.json$/.test(name)) continue;
      let j;
      try { j = JSON.parse(fs.readFileSync(path.join(this.registryDir, name), 'utf8')); } catch { continue; }
      const sid = sanitize(j && j.sessionId);
      if (!sid || !pidAlive(j.pid)) continue;
      seen.add(sid);
      const state = this.records.get(sid)?.state;
      const name_ = typeof j.name === 'string' ? j.name.slice(0, 120) : '';
      if (!state || !state.inRegistry || state.pid !== j.pid || (name_ && state.regName !== name_)) {
        this.append(sid, { ev: 'Reg', pid: j.pid, rname: name_ || undefined, cwd: j.cwd, ep: j.entrypoint, kind: j.kind });
      }
      const status = typeof j.status === 'string' ? j.status : '';
      if (status && this.records.get(sid)?.state.regStatus !== status) {
        const waitingFor = status === 'waiting' && typeof j.waitingFor === 'string' ? j.waitingFor.slice(0, 120) : undefined;
        this.append(sid, { ev: 'RegStatus', status, waitingFor, t: Number(j.statusUpdatedAt) || now });
      }
    }
    for (const [sid, rec] of this.records) {
      if (rec.state.inRegistry && !seen.has(sid)) this.append(sid, { ev: 'Gone' });
    }
    if (first) this.dirty = true;
  }

  clear() {
    let names = [];
    try { names = fs.readdirSync(this.eventsDir); } catch {}
    for (const name of names) {
      if (name.endsWith('.jsonl')) { try { fs.unlinkSync(path.join(this.eventsDir, name)); } catch {} }
    }
    this.records.clear();
    this.offsets.clear();
    this.pollRegistry(true);
    this.flush();
  }

  snapshot(now = Date.now()) {
    const views = [];
    for (const rec of this.records.values()) views.push(reducer.view(rec.state, now, { readyTtlMin: this.readyTtlMin, showBackground: this.showBackground }));
    const sessions = reducer.sortViews(views.filter((v) => !v.hidden));
    return { sessions, overall: reducer.overall(views) };
  }

  get(sid) {
    return this.records.get(sanitize(sid))?.state || null;
  }

  livePids() {
    const pids = [];
    for (const rec of this.records.values()) if (rec.state.inRegistry && rec.state.pid) pids.push(rec.state.pid);
    return pids;
  }
}

module.exports = { SessionStore, sanitize };
