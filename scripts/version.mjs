#!/usr/bin/env node
/**
 * Reads or rewrites the application version — one command, four files.
 *
 *   node scripts/version.mjs          # print the current version
 *   node scripts/version.mjs 0.2.0    # bump every file that carries it
 *
 * `src-tauri/tauri.conf.json` is the source of truth (it is the version the
 * Tauri CLI stamps into the bundles); `package.json`, `src-tauri/Cargo.toml`
 * and the `mq-manager` entry in `src-tauri/Cargo.lock` are kept in sync so the
 * frontend bundle and the crate metadata never disagree.
 *
 * Bumping the version is what publishes a release — the release workflow on
 * `main` sees the new number, tags it and builds every platform. See AGENTS.md.
 *
 * Only the first `version` field of each file is rewritten, so the versions of
 * dependencies are never touched by accident.
 *
 * Run through `just app-version` / `just set-version <x.y.z>`.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Order matters: the first entry is the source of truth. */
export const VERSION_FILES = [
  { path: 'src-tauri/tauri.conf.json', pattern: /("version"\s*:\s*")([^"]+)(")/ },
  { path: 'package.json', pattern: /("version"\s*:\s*")([^"]+)(")/ },
  { path: 'src-tauri/Cargo.toml', pattern: /^(version\s*=\s*")([^"]+)(")/m },
  { path: 'src-tauri/Cargo.lock', pattern: /(name = "mq-manager"\r?\nversion = ")([^"]+)(")/ },
];

const SEMVER = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

/** @returns {string} the version currently carried by the app config. */
export function readVersion() {
  const { path, pattern } = VERSION_FILES[0];
  const match = pattern.exec(readFileSync(join(ROOT, path), 'utf8'));
  if (!match) {
    throw new Error(`no version field found in ${path}`);
  }
  return match[2];
}

/**
 * Rewrites the version in every file that carries it.
 * @param {string} version
 * @returns {string[]} the files that changed, relative to the repository root.
 */
export function writeVersion(version) {
  if (!SEMVER.test(version)) {
    throw new Error(`\`${version}\` is not a semantic version (expected x.y.z)`);
  }

  const changed = [];
  for (const { path, pattern } of VERSION_FILES) {
    const file = join(ROOT, path);
    const before = readFileSync(file, 'utf8');
    if (!pattern.test(before)) {
      throw new Error(`no version field found in ${path}`);
    }
    const after = before.replace(pattern, `$1${version}$3`);
    if (after !== before) {
      writeFileSync(file, after);
      changed.push(relative(ROOT, file).replace(/\\/g, '/'));
    }
  }
  return changed;
}

function main(args) {
  const next = args.find((arg) => !arg.startsWith('-'));
  if (!next) {
    process.stdout.write(`${readVersion()}\n`);
    return;
  }

  const previous = readVersion();
  const changed = writeVersion(next);
  if (changed.length === 0) {
    process.stdout.write(`${previous} is already ${next} — nothing to do.\n`);
    return;
  }
  process.stdout.write(`${previous} → ${next}\n`);
  for (const file of changed) {
    process.stdout.write(`  updated ${file}\n`);
  }
  process.stdout.write(
    '\nNext: add a `## [' +
      next +
      ']` section to CHANGELOG.md, commit and push — the release\n' +
      'workflow tags the commit and builds the installers. See AGENTS.md.\n',
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
