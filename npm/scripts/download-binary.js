#!/usr/bin/env node
/**
 * Download the native DeepHarness binaries (`dh` and `dh-gatewayd`) from the
 * GitHub release that matches the current platform and architecture.
 */
import { existsSync, mkdirSync, writeFileSync, chmodSync, readFileSync } from 'fs';
import { dirname, join } from 'path';
import { homedir } from 'os';
import { fileURLToPath } from 'url';

const GITHUB_OWNER = 'WraithN';
const GITHUB_REPO = 'deepharness-ent-desktop';
const DOWNLOAD_TIMEOUT_MS = 60_000;

const DH_PLATFORM_ASSET_NAMES = {
  'linux:x64': 'dh-linux-x64',
  'linux:arm64': 'dh-linux-arm64',
  'darwin:x64': 'dh-darwin-x64',
  'darwin:arm64': 'dh-darwin-arm64',
  'win32:x64': 'dh-windows-x64.exe',
};

const GATEWAYD_PLATFORM_ASSET_NAMES = {
  'linux:x64': 'dh-gatewayd-linux-x64',
  'linux:arm64': 'dh-gatewayd-linux-arm64',
  'darwin:x64': 'dh-gatewayd-darwin-x64',
  'darwin:arm64': 'dh-gatewayd-darwin-arm64',
  'win32:x64': 'dh-gatewayd-windows-x64.exe',
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

function getAssetName(platformAssetNames) {
  const key = `${process.platform}:${process.arch}`;
  const assetName = platformAssetNames[key];
  if (!assetName) {
    throw new Error(`Unsupported platform/architecture: ${key}. Supported platforms: ${Object.keys(platformAssetNames).join(', ')}`);
  }
  return assetName;
}

function getDhBinaryName() {
  return process.platform === 'win32' ? 'dh.exe' : 'dh';
}

function getGatewaydBinaryName() {
  return process.platform === 'win32' ? 'dh-gatewayd.exe' : 'dh-gatewayd';
}

function getInstallDir() {
  return join(homedir(), '.local', 'bin');
}

function getDhBinaryPath() {
  return join(getInstallDir(), getDhBinaryName());
}

function getGatewaydBinaryPath() {
  return join(getInstallDir(), getGatewaydBinaryName());
}

function getDownloadUrl(version, assetName) {
  return `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/dh-v${version}/${assetName}`;
}

/**
 * Download a release asset and install it to the given path.
 */
async function downloadReleaseAsset(version, assetName, binaryPath, label) {
  const downloadUrl = getDownloadUrl(version, assetName);

  console.log(`[deepharness] Downloading ${label} ${version} for ${process.platform}-${process.arch}...`);
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

  console.log(`[deepharness] Installed ${label} to ${binaryPath}`);
  return binaryPath;
}

/**
 * Download the `dh` binary for the current platform and install it to ~/.local/bin.
 * Returns the path to the installed binary.
 */
export async function downloadDhBinary(version) {
  const assetName = getAssetName(DH_PLATFORM_ASSET_NAMES);
  const binaryPath = getDhBinaryPath();
  return downloadReleaseAsset(version, assetName, binaryPath, 'dh');
}

/**
 * Download the `dh-gatewayd` binary for the current platform and install it to ~/.local/bin.
 * Returns the path to the installed binary.
 */
export async function downloadGatewaydBinary(version) {
  const assetName = getAssetName(GATEWAYD_PLATFORM_ASSET_NAMES);
  const binaryPath = getGatewaydBinaryPath();
  return downloadReleaseAsset(version, assetName, binaryPath, 'dh-gatewayd');
}

/**
 * Check whether the installed `dh` binary is present.
 */
export function isBinaryInstalled() {
  return existsSync(getDhBinaryPath());
}

/**
 * Check whether the installed `dh-gatewayd` binary is present.
 */
export function isGatewaydBinaryInstalled() {
  return existsSync(getGatewaydBinaryPath());
}

export { getDhBinaryPath, getGatewaydBinaryPath, getPackageVersion };

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
