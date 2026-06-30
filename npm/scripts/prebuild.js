#!/usr/bin/env node
/**
 * Pre-build hook for the `deepharness` npm package.
 *
 * Before publishing the npm package, this script downloads or copies the
 * native `dh` binaries for all supported platforms into `npm/binaries/`.
 * The published package therefore contains all native binaries and does not
 * need a postinstall hook.
 */
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const GITHUB_OWNER = 'WraithN';
const GITHUB_REPO = 'deepharness-ent-desktop';
const DOWNLOAD_TIMEOUT_MS = 120_000;

const PLATFORMS = [
  { platform: 'linux', arch: 'x64', assetName: 'dh-linux-x64', binaryName: 'dh' },
  { platform: 'linux', arch: 'arm64', assetName: 'dh-linux-arm64', binaryName: 'dh' },
  { platform: 'darwin', arch: 'x64', assetName: 'dh-darwin-x64', binaryName: 'dh' },
  { platform: 'darwin', arch: 'arm64', assetName: 'dh-darwin-arm64', binaryName: 'dh' },
  { platform: 'win32', arch: 'x64', assetName: 'dh-windows-x64.exe', binaryName: 'dh.exe' },
];

function getRootDir() {
  return join(__dirname, '..');
}

function readPackageVersion() {
  const pkgPath = join(getRootDir(), 'package.json');
  try {
    const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
    return pkg.version;
  } catch {
    return null;
  }
}

function readCargoVersion() {
  const cargoPath = join(getRootDir(), '..', 'Cargo.toml');
  try {
    const cargo = readFileSync(cargoPath, 'utf8');
    const match = cargo.match(/^version\s*=\s*"([^"]+)"/m);
    return match ? match[1] : null;
  } catch {
    return null;
  }
}

function getReleaseVersion() {
  // Prefer Cargo workspace version since native binaries are versioned by Cargo.
  return readCargoVersion() || readPackageVersion();
}

function getBinaryDir() {
  return join(getRootDir(), 'binaries');
}

function getBinaryPath(platform, arch, binaryName) {
  const suffix = binaryName.endsWith('.exe') ? '.exe' : '';
  return join(getBinaryDir(), `dh-${platform}-${arch}${suffix}`);
}

function getDownloadUrl(version, assetName) {
  return `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/dh-v${version}/${assetName}`;
}

function getProxyUrl() {
  return (
    process.env.HTTPS_PROXY ||
    process.env.https_proxy ||
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    null
  );
}

async function fetchWithProxy(url) {
  const proxyUrl = getProxyUrl();
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), DOWNLOAD_TIMEOUT_MS);

  try {
    if (proxyUrl) {
      try {
        const { ProxyAgent } = await import('undici');
        return await fetch(url, {
          signal: controller.signal,
          dispatcher: new ProxyAgent(proxyUrl),
        });
      } catch (err) {
        console.warn(
          `[deepharness] Proxy download failed (${err.message}), retrying without proxy.`,
        );
      }
    }
    return await fetch(url, { signal: controller.signal });
  } finally {
    clearTimeout(timeoutId);
  }
}

async function downloadBinary(version, assetName, destPath) {
  const url = getDownloadUrl(version, assetName);
  console.log(`[deepharness] Downloading ${assetName} from ${url}...`);

  const response = await fetchWithProxy(url);
  if (!response.ok) {
    throw new Error(`Download failed: ${response.status} ${response.statusText}`);
  }

  const buffer = Buffer.from(await response.arrayBuffer());
  mkdirSync(dirname(destPath), { recursive: true });
  writeFileSync(destPath, buffer);
}

function copyLocalBinary(assetName, destPath) {
  const rootDir = join(getRootDir(), '..');
  const isWindows = assetName.endsWith('.exe');
  const localName = isWindows ? 'dh.exe' : 'dh';
  const candidates = [
    join(rootDir, 'dist', assetName),
    join(rootDir, 'target', 'release', localName),
  ];

  for (const src of candidates) {
    if (existsSync(src)) {
      mkdirSync(dirname(destPath), { recursive: true });
      copyFileSync(src, destPath);
      console.log(`[deepharness] Copied local binary ${src} -> ${destPath}`);
      return true;
    }
  }

  return false;
}

async function prepareBinary(platformInfo, version) {
  const { platform, arch, assetName, binaryName } = platformInfo;
  const destPath = getBinaryPath(platform, arch, binaryName);

  if (existsSync(destPath)) {
    console.log(`[deepharness] Binary already exists: ${destPath}`);
    return;
  }

  // Try downloading from GitHub release first.
  try {
    await downloadBinary(version, assetName, destPath);
    console.log(`[deepharness] Downloaded ${assetName} -> ${destPath}`);
    return;
  } catch (err) {
    console.warn(`[deepharness] Download failed for ${assetName}: ${err.message}`);
  }

  // Fallback: copy from local build artifacts.
  if (copyLocalBinary(assetName, destPath)) {
    return;
  }

  throw new Error(`Could not prepare binary for ${platform}-${arch}`);
}

async function main() {
  const version = getReleaseVersion();
  if (!version) {
    console.error('[deepharness] Could not determine release version.');
    process.exit(1);
  }

  console.log(`[deepharness] Pre-building npm package for dh v${version}...`);

  mkdirSync(getBinaryDir(), { recursive: true });

  let hasError = false;
  for (const platformInfo of PLATFORMS) {
    try {
      await prepareBinary(platformInfo, version);
      const destPath = getBinaryPath(
        platformInfo.platform,
        platformInfo.arch,
        platformInfo.binaryName,
      );
      if (process.platform !== 'win32' && !platformInfo.binaryName.endsWith('.exe')) {
        chmodSync(destPath, 0o755);
      }
    } catch (err) {
      console.error(
        `[deepharness] Failed to prepare ${platformInfo.assetName}: ${err.message}`,
      );
      hasError = true;
    }
  }

  if (hasError) {
    console.error('[deepharness] Pre-build completed with errors.');
    process.exit(1);
  }

  console.log('[deepharness] Pre-build completed successfully.');
}

main().catch((err) => {
  console.error(`[deepharness] Pre-build failed: ${err.message}`);
  process.exit(1);
});
