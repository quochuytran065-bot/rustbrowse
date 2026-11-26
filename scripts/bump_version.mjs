#!/usr/bin/env node
/**
 * Atomic Semantic Version Synchronizer & Validator for RustBrowse
 * Keeps package.json, src-tauri/Cargo.toml, Cargo.toml, src-tauri/tauri.conf.json,
 * and CHANGELOG.md 100% synchronized.
 *
 * Usage:
 *   node scripts/bump_version.mjs --check [--expect 1.0.1]
 *   node scripts/bump_version.mjs patch | minor | major | 1.0.0
 */

import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');

const FILES = {
  pkg: path.join(ROOT, 'package.json'),
  tauriCargo: path.join(ROOT, 'src-tauri', 'Cargo.toml'),
  rootCargo: path.join(ROOT, 'Cargo.toml'),
  tauriConf: path.join(ROOT, 'src-tauri', 'tauri.conf.json'),
  changelog: path.join(ROOT, 'CHANGELOG.md'),
};

const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

function readVersions() {
  const pkg = JSON.parse(fs.readFileSync(FILES.pkg, 'utf8'));
  const tauriConf = JSON.parse(fs.readFileSync(FILES.tauriConf, 'utf8'));
  const tauriCargoRaw = fs.readFileSync(FILES.tauriCargo, 'utf8');
  const mTauri = tauriCargoRaw.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!mTauri) throw new Error('Could not parse [package].version in src-tauri/Cargo.toml');

  const result = {
    'package.json': pkg.version,
    'src-tauri/tauri.conf.json': tauriConf.version,
    'src-tauri/Cargo.toml': mTauri[1],
  };

  if (fs.existsSync(FILES.rootCargo)) {
    const rootRaw = fs.readFileSync(FILES.rootCargo, 'utf8');
    const mRoot = rootRaw.match(/^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
    if (mRoot) result['Cargo.toml'] = mRoot[1];
  }
  return result;
}

function computeNextVersion(current, spec) {
  if (SEMVER_RE.test(spec)) return spec;
  const m = current.match(SEMVER_RE);
  if (!m) throw new Error(`Invalid current SemVer: ${current}`);
  let [major, minor, patch] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (spec === 'major') {
    major += 1;
    minor = 0;
    patch = 0;
  } else if (spec === 'minor') {
    minor += 1;
    patch = 0;
  } else if (spec === 'patch') {
    patch += 1;
  } else {
    throw new Error(`Unknown version bump argument: "${spec}". Expected patch|minor|major|X.Y.Z|--check`);
  }
  return `${major}.${minor}.${patch}`;
}

function updateChangelog(newVersion) {
  const date = new Date().toISOString().slice(0, 10);
  let commitLines = '- Automated release build and stability improvements.';
  try {
    let range = '-n 15';
    try {
      const lastTag = execSync('git describe --tags --abbrev=0', { cwd: ROOT, stdio: ['ignore', 'pipe', 'ignore'] })
        .toString()
        .trim();
      if (lastTag) range = `${lastTag}..HEAD`;
    } catch {}
    const log = execSync(`git log ${range} --pretty=format:"- %s"`, {
      cwd: ROOT,
      stdio: ['ignore', 'pipe', 'ignore'],
    })
      .toString()
      .trim();
    if (log) commitLines = log;
  } catch {}

  const entry = `## [${newVersion}] - ${date}\n\n${commitLines}\n\n`;
  if (fs.existsSync(FILES.changelog)) {
    const existing = fs.readFileSync(FILES.changelog, 'utf8');
    if (existing.includes(`## [${newVersion}]`)) return;
    if (existing.startsWith('# ')) {
      const idx = existing.indexOf('\n');
      const updated = existing.slice(0, idx + 1) + '\n' + entry + existing.slice(idx + 1).replace(/^\n+/, '');
      fs.writeFileSync(FILES.changelog, updated, 'utf8');
      return;
    }
    fs.writeFileSync(FILES.changelog, `# Changelog\n\n${entry}${existing}`, 'utf8');
  } else {
    fs.writeFileSync(FILES.changelog, `# Changelog\n\n${entry}`, 'utf8');
  }
}

const args = process.argv.slice(2);
if (args.includes('--check')) {
  const versions = readVersions();
  const unique = [...new Set(Object.values(versions))];
  const expectIdx = args.indexOf('--expect');
  const expected = expectIdx !== -1 ? args[expectIdx + 1]?.replace(/^v/, '') : null;

  for (const [file, ver] of Object.entries(versions)) {
    if (!SEMVER_RE.test(ver)) {
      console.error(`ERROR: Invalid SemVer "${ver}" in ${file}`);
      process.exit(1);
    }
  }
  if (unique.length !== 1) {
    console.error('ERROR: Version mismatch across manifests:', versions);
    process.exit(1);
  }
  if (expected && unique[0] !== expected) {
    console.error(`ERROR: Manifest version (${unique[0]}) does not match expected tag version (${expected})`);
    process.exit(1);
  }
  console.log(`OK: All manifests synchronized at v${unique[0]}`);
  process.exit(0);
}

const spec = args[0];
if (!spec) {
  console.error('Usage: node scripts/bump_version.mjs <patch|minor|major|X.Y.Z|--check>');
  process.exit(1);
}

const currentMap = readVersions();
const current = currentMap['src-tauri/tauri.conf.json'];
const next = computeNextVersion(current, spec.replace(/^v/, ''));

// 1. package.json
const pkgJson = JSON.parse(fs.readFileSync(FILES.pkg, 'utf8'));
pkgJson.version = next;
fs.writeFileSync(FILES.pkg, JSON.stringify(pkgJson, null, 2) + '\n', 'utf8');

// 2. src-tauri/tauri.conf.json
const tauriJson = JSON.parse(fs.readFileSync(FILES.tauriConf, 'utf8'));
tauriJson.version = next;
fs.writeFileSync(FILES.tauriConf, JSON.stringify(tauriJson, null, 2) + '\n', 'utf8');

// 3. src-tauri/Cargo.toml
const tauriCargo = fs.readFileSync(FILES.tauriCargo, 'utf8');
const updatedTauriCargo = tauriCargo.replace(
  /(^\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m,
  `$1${next}$3`
);
fs.writeFileSync(FILES.tauriCargo, updatedTauriCargo, 'utf8');

// 4. Root Cargo.toml (if present)
if (fs.existsSync(FILES.rootCargo)) {
  const rootCargo = fs.readFileSync(FILES.rootCargo, 'utf8');
  const updatedRootCargo = rootCargo.replace(
    /(^\[workspace\.package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m,
    `$1${next}$3`
  );
  fs.writeFileSync(FILES.rootCargo, updatedRootCargo, 'utf8');
}

// 5. CHANGELOG.md
updateChangelog(next);

console.log(`Synchronized version: ${current} -> ${next}`);
