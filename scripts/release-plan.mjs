#!/usr/bin/env node
/**
 * Decides whether the current commit should publish a release, and drafts the
 * release message.
 *
 *   node scripts/release-plan.mjs                  # preview (human readable)
 *   node scripts/release-plan.mjs --github-output  # key=value for $GITHUB_OUTPUT
 *
 * A release is built when the version in `src-tauri/tauri.conf.json` changed in
 * this push and its tag does not exist yet, which keeps every other push to
 * `main` from re-releasing the same version. A manual run
 * (`workflow_dispatch`) releases the current version unless the tag is already
 * there.
 *
 * The message is the matching `CHANGELOG.md` section — falling back to
 * `[Unreleased]` — followed by the commit subjects since the previous tag, so a
 * release always explains what changed without anyone writing it twice.
 */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { readVersion } from './version.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MAX_COMMITS = 50;
const CONFIG_PATH = 'src-tauri/tauri.conf.json';

function git(...args) {
  const result = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  return result.status === 0 ? result.stdout.trim() : null;
}

/** The version recorded in a revision, or null when it cannot be read. */
function versionAt(revision) {
  if (!revision) return null;
  const raw = git('show', `${revision}:${CONFIG_PATH}`);
  if (!raw) return null;
  try {
    return JSON.parse(raw).version ?? null;
  } catch {
    return null;
  }
}

/**
 * The `## [version]` section of CHANGELOG.md, `[Unreleased]` as a fallback.
 * @returns {{ body: string, versioned: boolean } | null}
 */
function changelogSection(version) {
  let text;
  try {
    text = readFileSync(join(ROOT, 'CHANGELOG.md'), 'utf8');
  } catch {
    return null;
  }

  const lines = text.split(/\r?\n/);
  for (const [index, heading] of [`## [${version}]`, '## [Unreleased]'].entries()) {
    const start = lines.findIndex((line) => line.trim().startsWith(heading));
    if (start === -1) continue;

    let end = lines.length;
    for (let line = start + 1; line < lines.length; line += 1) {
      // The next section, or the link definitions keep-a-changelog puts last.
      if (/^##\s/.test(lines[line]) || /^\[[^\]]+\]:/.test(lines[line])) {
        end = line;
        break;
      }
    }
    const body = lines
      .slice(start + 1, end)
      .join('\n')
      .trim();
    if (body) {
      return { body, versioned: index === 0 };
    }
  }
  return null;
}

function commitLog(tag) {
  const previous = git('describe', '--tags', '--abbrev=0', '--match', 'v*', '--exclude', tag);
  const range = previous ? `${previous}..HEAD` : 'HEAD';
  return git('log', `-n${MAX_COMMITS}`, '--no-merges', '--pretty=format:- %s (%h)', range) ?? '';
}

function tagExists(tag) {
  if (git('tag', '--list', tag)) return true;
  // `ls-remote` is the only check that sees tags pushed by somebody else.
  return git('ls-remote', '--exit-code', '--tags', 'origin', `refs/tags/${tag}`) !== null;
}

function buildNotes(version, tag) {
  const parts = [`## MQ Manager ${tag}`];

  const section = changelogSection(version);
  if (!section) {
    parts.push(`_No CHANGELOG.md entry for ${version} yet._`);
  } else if (section.versioned) {
    parts.push(section.body);
  } else {
    parts.push(
      `_CHANGELOG.md has no \`[${version}]\` section yet, so this is the current ` +
        `\`[Unreleased]\` entry._\n\n${section.body}`,
    );
  }

  const commits = commitLog(tag);
  if (commits) {
    parts.push(`### Commits\n\n${commits}`);
  }
  return `${parts.join('\n\n')}\n`;
}

function plan() {
  const version = readVersion();
  const tag = `v${version}`;
  const manual = process.env.GITHUB_EVENT_NAME === 'workflow_dispatch';

  if (tagExists(tag)) {
    return { release: false, version, tag, reason: `${tag} already exists — nothing to release.` };
  }

  if (!manual) {
    const before = versionAt(process.env.BEFORE_SHA);
    if (before === version) {
      return {
        release: false,
        version,
        tag,
        reason: `the version did not change in this push (still ${version}).`,
      };
    }
    return {
      release: true,
      version,
      tag,
      reason:
        before === null
          ? 'no previous version to compare with.'
          : `the version changed in this push (${before} → ${version}).`,
      notes: buildNotes(version, tag),
    };
  }

  return {
    release: true,
    version,
    tag,
    reason: 'manual run.',
    notes: buildNotes(version, tag),
  };
}

const result = plan();
const wantsOutput = process.argv.includes('--github-output');

if (wantsOutput) {
  const lines = [
    `release=${result.release}`,
    `version=${result.version}`,
    `tag=${result.tag}`,
    `reason=${result.reason}`,
  ];
  if (result.notes) {
    // Multi-line values need the heredoc form of $GITHUB_OUTPUT.
    lines.push(`notes<<NOTES_EOF`, result.notes.replace(/\n$/, ''), 'NOTES_EOF');
  } else {
    lines.push('notes=');
  }
  process.stdout.write(`${lines.join('\n')}\n`);
} else {
  process.stdout.write(`version : ${result.version}\n`);
  process.stdout.write(`tag     : ${result.tag}\n`);
  process.stdout.write(`release : ${result.release ? 'yes' : 'no'}\n`);
  process.stdout.write(`reason  : ${result.reason}\n`);
  if (result.notes) {
    process.stdout.write(`\n${result.notes}`);
  }
}
