#!/usr/bin/env node
/**
 * Download the native `dh` binary from the GitHub release that matches the
 * current platform and architecture.
 */
import { existsSync, mkdirSync, writeFileSync, chmodSync, readFileSync } from 'fs';
import { dirname, join } from 'path';
import { homedir } from 'os';
import { fileURLToPath } from 'url';

const GITHUB_OWNER = 'WraithN';
const GITHUB_REPO = 'deepharness-ent-desktop';
const DOWNLOAD_TIMEOUT_MS = 60_000;

const PLATFORM_ASSET_NAMES = {
  'linux:x64': 'dh-linux-x64',
  'linux:arm64': 'dh-linux-arm64',
  'darwin:x64': 'dh-darwin-x64',
  'darwin:arm64': 'dh-darwin-arm64',
  'win32:x64': 'dh-windows-x64.exe',
};

function getProxyUrl() {
  return process.env.HTTPS_PROXY || process.env.https_proxy || process.env.HTTP_PROXY || process.env.http_proxy || null;
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
        // If undici/ProxyAgent fails, fall back to default fetch so users
        // without a problematic proxy still work.
        console.warn(`[deepharness] Proxy download failed (${err.message}), retrying without proxy.`);
      }
    }

    return await fetch(url, { signal: controller.signal });
  } finally {
    clearTimeout(timeoutId);
  }
}

function getPackageVersion() {
  const __filename = fileURLToPath(import.meta.url);
  const packageJsonPath = join(dirname(__filename), '..', 'package.json');
  try {
    const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
    return pkg.version;
  } catch {
    return null;
  }
}

function getAssetName() {
  const key = `${process.platform}:${process.arch}`;
  const assetName = PLATFORM_ASSET_NAMES[key];
  if (!assetName) {
    throw new Error(`Unsupported platform/architecture: ${key}. Supported platforms: ${Object.keys(PLATFORM_ASSET_NAMES).join(', ')}`);
  }
  return assetName;
}

function getBinaryName() {
  return process.platform === 'win32' ? 'dh.exe' : 'dh';
}

function getInstallDir() {
  return join(homedir(), '.local', 'bin');
}

function getBinaryPath() {
  return join(getInstallDir(), getBinaryName());
}

function getDownloadUrl(version, assetName) {
  return `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/dh-v${version}/${assetName}`;
}

/**
 * Download the binary for the current platform and install it to ~/.local/bin.
 * Returns the path to the installed binary.
 */
export async function downloadDhBinary(version) {
  const assetName = getAssetName();
  const binaryPath = getBinaryPath();
  const downloadUrl = getDownloadUrl(version, assetName);

  console.log(`[deepharness] Downloading dh ${version} for ${process.platform}-${process.arch}...`);
  console.log(`[deepharness] URL: ${downloadUrl}`);

  const response = await fetchWithProxy(downloadUrl);
  if (!response.ok) {
    throw new Error(`Download failed: ${response.status} ${response.statusText} (${downloadUrl})`);
  }

  const buffer = Buffer.from(await response.arrayBuffer());

  mkdirSync(getInstallDir(), { recursive: true });
  writeFileSync(binaryPath, buffer);

  if (process.platform !== 'win32') {
    chmodSync(binaryPath, 0o755);
  }

  console.log(`[deepharness] Installed dh to ${binaryPath}`);
  return binaryPath;
}

/**
 * Check whether the installed binary is present.
 */
export function isBinaryInstalled() {
  return existsSync(getBinaryPath());
}

export { getBinaryPath, getPackageVersion };

// CLI entry point for testing or manual download.
if (import.meta.url === `file://${process.argv[1]}`) {
  const version = process.argv[2] || getPackageVersion();
  if (!version) {
    console.error('Usage: node download-binary.js <version>');
    process.exit(1);
  }
  downloadDhBinary(version).catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
