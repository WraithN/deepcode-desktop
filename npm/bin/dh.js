#!/usr/bin/env node
import { spawn, execSync } from 'child_process';
import { existsSync, realpathSync, readFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { homedir } from 'os';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const SYSTEM_INSTALL_URL = 'https://github.com/WraithN/deepharness-ent-desktop';

/**
 * Resolve the real path of the wrapper script, following symlinks created by
 * `npm link` or package managers.
 */
function resolveWrapperPath() {
  try {
    return realpathSync(__filename);
  } catch {
    return __filename;
  }
}

const WRAPPER_PATH = resolveWrapperPath();

/**
 * Check whether a candidate path resolves to this wrapper script itself.
 * This prevents `which dh` from returning the npm-installed JS wrapper and
 * causing an infinite subprocess loop.
 */
function isWrapperItself(candidate) {
  try {
    return realpathSync(candidate) === WRAPPER_PATH;
  } catch {
    return false;
  }
}

/**
 * Find the project root when the npm package is installed directly inside the
 * source repository (development / `npm link`). Returns `null` when installed
 * from the registry.
 */
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

/**
 * Return the bundled native binary for the current platform, if it exists.
 * The npm package is published with pre-built binaries in `binaries/`.
 */
function findBundledBinary() {
  const key = `${process.platform}:${process.arch}`;
  const bundledName = {
    'linux:x64': 'dh-linux-x64',
    'linux:arm64': 'dh-linux-arm64',
    'darwin:x64': 'dh-darwin-x64',
    'darwin:arm64': 'dh-darwin-arm64',
    'win32:x64': 'dh-win32-x64.exe',
  }[key];

  if (!bundledName) {
    return null;
  }

  const bundledPath = join(__dirname, '..', 'binaries', bundledName);
  if (existsSync(bundledPath)) {
    return bundledPath;
  }

  return null;
}

/**
 * Build a list of candidate paths where the `dh` binary may live.
 */
function buildSearchPaths() {
  const paths = [];

  // 1. Explicit override from environment.
  if (process.env.DH_BINARY_PATH) {
    paths.push(resolve(process.env.DH_BINARY_PATH));
  }

  // 2. Bundled binary shipped with the npm package.
  try {
    const bundled = findBundledBinary();
    if (bundled) {
      paths.push(bundled);
    }
  } catch {}

  // 3. Project-local builds when developing from source.
  try {
    const projectRoot = findProjectRoot();
    if (projectRoot) {
      paths.push(join(projectRoot, 'target', 'release', 'dh'));
      paths.push(join(projectRoot, 'target', 'debug', 'dh'));
    }
  } catch {}

  // 4. Common user-level install locations.
  paths.push(
    join(homedir(), '.local', 'bin', 'dh'),
    join(homedir(), '.cargo', 'bin', 'dh'),
  );

  // 5. System-wide install locations.
  paths.push('/usr/local/bin/dh', '/usr/bin/dh');

  return paths;
}

/**
 * Locate the `dh` binary, or return `null` if it cannot be found.
 */
function findDhBinary() {
  for (const p of buildSearchPaths()) {
    if (existsSync(p) && !isWrapperItself(p)) return p;
  }

  // Fallback: rely on the user's PATH. Skip the fallback when we are already
  // inside a wrapper-spawned process to avoid infinite recursion if `which dh`
  // points back at this wrapper.
  if (process.env.DH_NPM_WRAPPER === '1') {
    return null;
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

function printInstallInstructions() {
  console.error('Error: `dh` binary not found.');
  console.error('');
  console.error('The `deepharness` npm package is a thin wrapper around the native `dh` binary.');
  console.error('Please install the binary using one of the following methods:');
  console.error('');
  console.error('  1. Install DeepHarness Desktop:');
  console.error(`     ${SYSTEM_INSTALL_URL}`);
  console.error('');
  console.error('  2. Build and install from source (requires Rust):');
  console.error('     cargo build --release -p deepharness-cli');
  console.error('     mkdir -p ~/.local/bin');
  console.error('     cp target/release/dh ~/.local/bin/dh');
  console.error('');
  console.error('  3. If the binary is already installed in a non-standard location:');
  console.error('     export DH_BINARY_PATH=/path/to/dh');
}

async function ensureDhBinary() {
  const existing = findDhBinary();
  if (existing) return existing;

  // Try to download the binary from GitHub release on first use.
  try {
    const { downloadDhBinary } = await import('../scripts/download-binary.js');
    const version = readPackageVersion();
    if (!version) {
      throw new Error('Cannot determine package version');
    }
    return await downloadDhBinary(version);
  } catch (err) {
    console.error(`[deepharness] Failed to download dh binary: ${err.message}`);
    return null;
  }
}

async function main() {
  const dhBin = await ensureDhBinary();

  if (!dhBin) {
    printInstallInstructions();
    process.exit(1);
  }

  const args = process.argv.slice(2);
  const proc = spawn(dhBin, args, {
    stdio: 'inherit',
    cwd: process.cwd(),
    env: { ...process.env, DH_NPM_WRAPPER: '1' },
  });

  proc.on('exit', (code) => process.exit(code ?? 1));
  proc.on('error', (err) => {
    console.error(`Failed to start dh from ${dhBin}:`, err.message);
    process.exit(1);
  });
}

main();
