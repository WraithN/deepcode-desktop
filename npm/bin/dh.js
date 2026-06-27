#!/usr/bin/env node
import { spawn, execSync } from 'child_process';
import { existsSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { homedir } from 'os';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function findProjectRoot() {
  let current = __dirname;
  while (true) {
    if (existsSync(join(current, 'Cargo.toml')) && existsSync(join(current, 'package.json'))) {
      return current;
    }
    const parent = dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

function findDhBinary() {
  const searchPaths = [];

  try {
    const projectRoot = findProjectRoot();
    if (projectRoot) {
      searchPaths.push(join(projectRoot, 'target', 'release', 'dh'));
      searchPaths.push(join(projectRoot, 'target', 'debug', 'dh'));
    }
  } catch {}

  searchPaths.push(
    '/usr/local/bin/dh',
    '/usr/bin/dh',
    join(homedir(), '.local', 'bin', 'dh'),
    join(homedir(), '.cargo', 'bin', 'dh'),
  );

  for (const p of searchPaths) {
    if (existsSync(p)) return p;
  }

  try {
    const which = execSync('which dh', { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    if (which && existsSync(which)) return which;
  } catch {}

  return null;
}

let dhBin = findDhBinary();

if (!dhBin) {
  console.error('Error: `dh` binary not found.');
  console.error('');
  console.error('Please install DeepHarness Desktop from: https://github.com/deepharness/deepharness-ent-desktop');
  console.error('Or build from source: cargo install --path apps/cli');
  process.exit(1);
}

const args = process.argv.slice(2);
const proc = spawn(dhBin, args, {
  stdio: 'inherit',
  cwd: process.cwd(),
  env: { ...process.env, DH_NPM_WRAPPER: '1' },
});

proc.on('exit', (code) => process.exit(code));
proc.on('error', (err) => {
  console.error('Failed to start dh:', err.message);
  process.exit(1);
});
