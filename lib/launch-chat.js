'use strict';
const fs = require('fs');
const { spawnSync } = require('child_process');

const file = process.argv[2];
let job;
try {
  job = JSON.parse(fs.readFileSync(file, 'utf8'));
} catch {
  console.error('Nibble: could not read the chat request.');
  process.exit(1);
} finally {
  fs.rmSync(file, { force: true });
}

const result = spawnSync(job.bin, [job.prompt], { stdio: 'inherit' });
process.exit(result.status ?? 0);
