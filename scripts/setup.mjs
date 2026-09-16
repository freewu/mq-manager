#!/usr/bin/env node
/**
 * Environment bootstrap and doctor for MQ Manager.
 *
 *   node scripts/setup.mjs           # check the toolchain, then wire up the
 *                                    # native build environment (.cargo/config.toml)
 *   node scripts/setup.mjs --check   # report only, never writes anything
 *
 * Why this exists: the Kafka driver compiles `librdkafka` from source through
 * the `cmake` crate. Most Windows machines that have Visual Studio installed do
 * have CMake, but only inside the VS installation directory — it is not on
 * PATH, and the `cmake` crate cannot find it. This script locates a usable
 * CMake and pins it through the (gitignored) `.cargo/config.toml`.
 *
 * Zero dependencies on purpose: it must be runnable before `pnpm install`.
 */

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const checkOnly = process.argv.includes('--check') || process.argv.includes('--doctor');
const isWindows = process.platform === 'win32';

/* -------------------------------------------------------------------------- */
/* tiny terminal helpers                                                      */
/* -------------------------------------------------------------------------- */

const useColor = process.stdout.isTTY && !process.env.NO_COLOR;
const paint = (code, text) => (useColor ? `\u001b[${code}m${text}\u001b[0m` : text);
const green = (text) => paint(32, text);
const red = (text) => paint(31, text);
const yellow = (text) => paint(33, text);
const dim = (text) => paint(2, text);
const bold = (text) => paint(1, text);

function heading(text) {
  process.stdout.write(`\n${bold(text)}\n`);
}

/* -------------------------------------------------------------------------- */
/* process helpers                                                            */
/* -------------------------------------------------------------------------- */

function run(command, args = []) {
  // `.cmd` / `.bat` shims (pnpm, npm, corepack) only run through a shell, and
  // paths containing spaces must be quoted by hand — spawn does not escape
  // arguments when `shell` is enabled.
  const needsShell = isWindows && /\.(cmd|bat)$/i.test(command);
  const target = needsShell && command.includes(' ') ? `"${command}"` : command;

  const result = spawnSync(target, args, {
    encoding: 'utf8',
    windowsHide: true,
    shell: needsShell,
  });
  if (result.error || result.status !== 0) return null;
  return (result.stdout ?? '').trim();
}

function firstLine(text) {
  return text ? text.split(/\r?\n/)[0].trim() : null;
}

/** Absolute path of an executable, resolved through `where` / `which`. */
function findExecutable(name) {
  const finder = isWindows ? 'where' : 'which';
  const found = run(finder, [name]);
  if (!found) return null;

  const candidates = found
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && existsSync(line));

  if (!isWindows) return candidates[0] ?? null;

  // `where` lists the extension-less shell script of npm-installed tools
  // before the `.cmd` shim; only the shim is executable from Node.
  const rank = (value) => {
    const extension = extname(value).toLowerCase();
    if (extension === '.exe') return 0;
    if (extension === '.cmd') return 1;
    if (extension === '.bat') return 2;
    return 3;
  };

  candidates.sort((left, right) => rank(left) - rank(right));
  return candidates[0] ?? null;
}

/** Runs `<tool> --version` style probes, falling back to PATH lookup. */
function probe(name, args = ['--version']) {
  const resolved = findExecutable(name) ?? name;
  return firstLine(run(resolved, args));
}

const results = [];

function record(name, ok, detail, hint) {
  results.push({ name, ok, detail, hint });
  const mark = ok === true ? green('  ok  ') : ok === 'warn' ? yellow(' warn ') : red(' fail ');
  const suffix = detail ? dim(`  ${detail}`) : '';
  process.stdout.write(`${mark} ${name.padEnd(22)}${suffix}\n`);
  if (ok === false && hint) process.stdout.write(`${' '.repeat(8)}${yellow(hint)}\n`);
}

/* -------------------------------------------------------------------------- */
/* toolchain checks                                                           */
/* -------------------------------------------------------------------------- */

function checkNode() {
  const version = firstLine(run(process.execPath, ['-v']));  const major = Number((version ?? 'v0').replace(/^v/, '').split('.')[0]);
  const minor = Number((version ?? 'v0.0').replace(/^v/, '').split('.')[1] ?? 0);
  const ok = major > 20 || (major === 20 && minor >= 19);
  record('node', ok, version ?? 'not found', 'Install Node.js 20.19+ — https://nodejs.org');
  return ok;
}

function checkPnpm() {
  const version = probe('pnpm');
  const major = Number((version ?? '0').split('.')[0]);
  const ok = Boolean(version) && major >= 9;
  record(
    'pnpm',
    ok,
    version ?? 'not found',
    'Install pnpm 9+:  corepack enable  (or: npm i -g pnpm)',
  );
  return ok;
}

function checkRust() {
  const cargo = probe('cargo', ['-V']);
  const rustc = run(findExecutable('rustc') ?? 'rustc', ['-vV']);
  const host = rustc ? /host:\s*(\S+)/.exec(rustc)?.[1] : null;
  const ok = Boolean(cargo);
  record('cargo', ok, cargo ?? 'not found', 'Install Rust — https://rustup.rs');
  if (rustc) record('rustc host', true, host ?? 'unknown');
  return ok;
}

function checkJust() {
  const version = probe('just');
  const ok = Boolean(version);
  record(
    'just',
    ok ? true : 'warn',
    version ?? 'not found',
    'Optional, but it is the documented entry point: cargo install just',
  );
  return ok;
}

function checkDocker() {
  const version = probe('docker');
  const ok = Boolean(version);
  record(
    'docker',
    ok ? true : 'warn',
    version ?? 'not found',
    'Only needed for `just kafka-up` (a local Kafka to develop against).',
  );
  return ok;
}

function checkCCompiler() {
  if (isWindows) {
    const cl = findExecutable('cl');
    const vs = visualStudioInstalls();
    const ok = Boolean(cl) || vs.length > 0;
    record(
      'MSVC build tools',
      ok ? true : 'warn',
      cl ?? (vs.length > 0 ? `found in ${vs[0]}` : 'not detected'),
      'Install "Desktop development with C++" — https://visualstudio.microsoft.com/visual-cpp-build-tools/',
    );
    return ok;
  }

  const compiler = findExecutable('cc') ?? findExecutable('clang') ?? findExecutable('gcc');
  const perl = findExecutable('perl');
  const nasm = findExecutable('nasm');
  record('C compiler', Boolean(compiler) ? true : false, compiler ?? 'not found', 'Install build-essential (or Xcode command line tools).');
  if (perl || nasm) record('perl / nasm', true, [perl && 'perl', nasm && 'nasm'].filter(Boolean).join(' + '));
  return Boolean(compiler);
}

/* -------------------------------------------------------------------------- */
/* CMake discovery                                                            */
/* -------------------------------------------------------------------------- */

function visualStudioInstalls() {
  if (!isWindows) return [];

  const programFilesX86 = process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)';
  const vswhere = join(programFilesX86, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
  if (!existsSync(vswhere)) return [];

  const output = run(vswhere, ['-products', '*', '-property', 'installationPath']);  return output ? output.split(/\r?\n/).map((line) => line.trim()).filter(Boolean) : [];
}

/** CMake bundled with Visual Studio lives under Common7/IDE. */
function vsCmakeCandidates() {
  return visualStudioInstalls().map((install) =>
    join(install, 'Common7', 'IDE', 'CommonExtensions', 'Microsoft', 'CMake', 'CMake', 'bin', 'cmake.exe'),
  );
}

function standaloneCmakeCandidates() {
  if (!isWindows) return [];
  return [
    'C:\\Program Files\\CMake\\bin\\cmake.exe',
    'C:\\Program Files (x86)\\CMake\\bin\\cmake.exe',
  ];
}

function detectCmake() {
  const onPath = findExecutable('cmake');
  if (onPath) return { path: onPath, source: 'PATH' };

  for (const candidate of [...standaloneCmakeCandidates(), ...vsCmakeCandidates()]) {
    if (existsSync(candidate)) {
      return {
        path: candidate,
        source: candidate.includes('CommonExtensions') ? 'Visual Studio' : 'standalone install',
      };
    }
  }

  return null;
}

/* -------------------------------------------------------------------------- */
/* .cargo/config.toml                                                         */
/* -------------------------------------------------------------------------- */

const tomlPath = (value) => value.replace(/\\/g, '/');

function writeCargoConfig(cmake) {
  const configDir = join(root, '.cargo');
  const configFile = join(configDir, 'config.toml');

  const content = `# Generated by \`just setup\` (scripts/setup.mjs) — do not commit.
#
# librdkafka is compiled from source by the \`cmake\` crate, which needs a CMake
# binary it cannot always find on its own (Visual Studio keeps its copy outside
# of PATH). Pinning it here keeps plain \`cargo build\` working.
[env]
CMAKE = "${tomlPath(cmake.path)}"
`;

  const previous = existsSync(configFile) ? readFileSync(configFile, 'utf8') : null;
  if (previous === content) {
    record('cargo config', true, '.cargo/config.toml already up to date');
    return;
  }

  mkdirSync(configDir, { recursive: true });
  writeFileSync(configFile, content, 'utf8');
  record('cargo config', true, `wrote .cargo/config.toml (CMAKE=${tomlPath(cmake.path)})`);
}

/* -------------------------------------------------------------------------- */
/* main                                                                       */
/* -------------------------------------------------------------------------- */

heading('MQ Manager — environment check');

const required = [checkNode(), checkPnpm(), checkRust()];
checkJust();
checkCCompiler();

heading('Native build dependencies');

const cmake = detectCmake();
if (cmake) {
  const version = firstLine(run(cmake.path, ['--version']))?.replace(/^cmake version\s*/i, '');  record('cmake', true, `${version ?? 'found'} (${cmake.source})`);

  if (checkOnly) {
    record('cargo config', 'warn', 'skipped (--check never writes)');
  } else {
    writeCargoConfig(cmake);
  }
} else {
  record(
    'cmake',
    false,
    'not found',
    isWindows
      ? 'Install CMake (or the VS "C++ CMake tools" component) so librdkafka can be compiled.'
      : 'Install cmake with your package manager (apt install cmake / brew install cmake).',
  );
}

heading('Optional');

const docker = checkDocker();
if (docker) record('kafka fixture', true, 'docker compose up -d kafka kafka-ui');

/* -------------------------------------------------------------------------- */
/* summary                                                                    */
/* -------------------------------------------------------------------------- */

const failures = results.filter((entry) => entry.ok === false);
const warnings = results.filter((entry) => entry.ok === 'warn');

heading('Summary');
process.stdout.write(
  `  ${results.length - failures.length - warnings.length} ok · ${warnings.length} warning(s) · ${failures.length} failure(s)\n`,
);

if (!required.every(Boolean)) {
  process.stdout.write(`\n${red('Fix the failures above and run `just setup` again.')}\n`);
  process.exit(1);
}

if (failures.length > 0) {
  process.stdout.write(
    `\n${yellow('The frontend can still run (`just web`), but the desktop build needs the missing tools above.')}\n`,
  );
}

process.stdout.write(`
Next steps:
  ${bold('pnpm install')}     install frontend dependencies
  ${bold('just dev')}         run the desktop app (compiles the Rust backend, ~2 min the first time)
  ${bold('just kafka-up')}    start a local single-node Kafka + UI
  ${bold('just doctor')}      re-run this report at any time

`);

void docker;
process.exit(failures.some((entry) => entry.name !== 'cmake' && entry.name !== 'docker') ? 1 : 0);
