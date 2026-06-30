#!/usr/bin/env node
/**
 * Post-install hook for the `deepharness` npm package.
 *
 * The npm package is only a wrapper; it needs the native `dh` binary to be
 * present on the system. This script verifies that the binary exists and,
 * when possible, installs it automatically by downloading from the matching
 * GitHub release or by building from source.
 */
import { existsSync, realpathSync, readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { homedir } from 'os';
import { execSync } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const SYSTEM_INSTALL_URL = 'https://github.com/WraithN/deepharness-ent-desktop';

function resolveWrapperPath() {
  try {
    return realpathSync(__filename);
  } catch {
    return __filename;
  }
}

const WRAPPER_PATH = resolveWrapperPath();

function isWrapperItself(candidate) {
  try {
    return realpathSync(candidate) === WRAPPER_PATH;
  } catch {
    return false;
  }
}

function findProjectRoot() {
  const wrapperPath = resolveWrapperPath();
  let current = dirname(wrapperPath);

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

function findExistingDhBinary() {
  const candidates = [
    join(homedir(), '.local', 'bin', 'dh'),
    join(homedir(), '.cargo', 'bin', 'dh'),
    '/usr/local/bin/dh',
    '/usr/bin/dh',
  ];

  for (const p of candidates) {
    if (existsSync(p) && !isWrapperItself(p)) return p;
  }

  try {
    const which = execSync('which dh', { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    if (which && existsSync(which) && !isWrapperItself(which)) return which;
  } catch {}

  return null;
}

function readPackageVersion() {
  try {
    const pkgPath = join(__dirname, '..', 'package.json');
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
    return pkg.version;
  } catch {
    return null;
  }
}

function buildFromSource(projectRoot) {
  console.log(`[deepharness] Building native dh binary from ${projectRoot}...`);
  try {
    execSync('cargo build --release -p deepharness-cli', {
      cwd: projectRoot,
      stdio: 'inherit',
    });
    return true;
  } catch {
    console.error('[deepharness] Failed to build dh from source.');
    return false;
  }
}

async function downloadBinary() {
  const { downloadDhBinary } = await import('./download-binary.js');
  const version = readPackageVersion();
  if (!version) {
    throw new Error('Cannot determine package version');
  }
  return downloadDhBinary(version);
}

async function main() {
  // If a usable binary already exists, nothing more to do.
  const existing = findExistingDhBinary();
  if (existing) {
    console.log(`[deepharness] Found dh binary at ${existing}`);
    return;
  }

  const projectRoot = findProjectRoot();

  // Prefer building from source when installed inside the repository.
  if (projectRoot) {
    console.log('[deepharness] dh binary not found; attempting to build from source...');
    const built = buildFromSource(projectRoot);
    if (built) {
      const releaseBinary = join(projectRoot, 'target', 'release', 'dh');
      if (existsSync(releaseBinary)) {
        console.log(`[deepharness] Built dh binary at ${releaseBinary}`);
        console.log('[deepharness] Add it to your PATH to use the `dh` command globally:');
        console.log(`  export PATH="${join(projectRoot, 'target', 'release')}:$PATH"`);
      }
      return;
    }
  }

  // Otherwise try to download the matching release binary.
  try {
    await downloadBinary();
    return;
  } catch (err) {
    console.warn(`[deepharness] Could not download binary: ${err.message}`);
  }

  console.warn('[deepharness] The native `dh` binary is not installed.');
  console.warn('[deepharness] The `dh` command will not work until it is available.');
  console.warn(`[deepharness] Install DeepHarness Desktop from: ${SYSTEM_INSTALL_URL}`);
  console.warn('[deepharness] Or build from source: cargo build --release -p deepharness-cli');
}

main().catch((err) => {
  console.warn(`[deepharness] Post-install hook failed: ${err.message}`);
});
