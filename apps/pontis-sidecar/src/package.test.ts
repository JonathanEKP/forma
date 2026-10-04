/**
 * What this package may reach, held by reading its sources rather than by a
 * comment saying so.
 */
import { strict as assert } from 'node:assert';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, it } from 'node:test';

const PACKAGE = resolve(import.meta.dirname, '..');
const REPO = resolve(PACKAGE, '../..');
const SOURCES = readdirSync(join(PACKAGE, 'src'))
  .filter((name) => name.endsWith('.ts'))
  .map((name) => ({ name, text: readFileSync(join(PACKAGE, 'src', name), 'utf8') }));

const PUBLIC_ENTRY_POINTS = ['@pontis/core', '@pontis/server', '@pontis/control-plane'];

function specifiers(text: string): string[] {
  return [...text.matchAll(/\bfrom\s+'([^']+)'/gu)].map((match) => match[1] ?? '');
}

describe('the package boundary', () => {
  it('imports Pontis only through its public package entry points', () => {
    for (const { name, text } of SOURCES) {
      for (const specifier of specifiers(text)) {
        if (specifier.includes('pontis') && !specifier.startsWith('@pontis/')) {
          assert.fail(`${name} reaches Pontis by path: ${specifier}`);
        }
        if (specifier.startsWith('@pontis/')) {
          assert.ok(PUBLIC_ENTRY_POINTS.includes(specifier), `${name} imports ${specifier}, which is not a public entry point`);
        }
      }
    }
  });

  it('keeps the FakeProvider driver out of everything but the tests', () => {
    // `stack.ts` is the tests' shared harness: it starts the driver, so it names it. What keeps that
    // from being a leak is the second half — nothing that is not a test, or the harness itself,
    // may import it.
    const support = ['stack.ts', 'ui-window.ts', 'browser-stack.ts'];
    const isTest = ({ name }: { name: string }) => name.endsWith('.test.ts') || support.includes(name);
    for (const { name, text } of SOURCES.filter((source) => !isTest(source))) {
      assert.ok(!text.includes('test-driver'), `${name} names the test driver`);
      for (const specifier of specifiers(text)) {
        assert.ok(!/\/(stack|ui-window)\.ts$/u.test(specifier), `${name} imports test support: ${specifier}`);
      }
    }
  });

  it('names the Pontis checkout in one seam and nowhere else', () => {
    // `.pontis` is the seam: a real distribution replaces it and nothing else.
    // These are the files that declare or document it; anything new naming it
    // is a second path to the checkout.
    const allowed = new Set([
      '.gitignore',
      'scripts/check.sh',
      'apps/pontis-sidecar/README.md',
      // This debt note documents the seam; prose is not a second resolution path.
      'docs/plan-status.md',
      // Task G1 records its required local symlink setup without changing the package's runtime seam.
      'docs/pontis-pv1-gaps.md',
      'apps/pontis-sidecar/package.json',
      'apps/pontis-sidecar/eslint.config.js',
      'apps/pontis-sidecar/test-driver/Cargo.toml',
      'apps/pontis-sidecar/src/package.test.ts',
      'pnpm-lock.yaml',
    ]);
    // The name standing alone, so `state.pontis_validations` is not a match.
    const naming = execFileSync('git', ['grep', '--untracked', '-l', '-E', '(^|[^A-Za-z0-9_])\\.pontis([^A-Za-z0-9_]|$)', '--', '.'], {
      cwd: REPO,
      encoding: 'utf8',
    })
      .split('\n')
      .filter(Boolean);
    for (const file of naming) assert.ok(allowed.has(file), `${file} names .pontis`);
    // Assembled, so this file does not match itself.
    const checkout = ['', 'projects', 'pontis'].join('/');
    for (const { name, text } of SOURCES) assert.ok(!text.includes(checkout), `${name} names a checkout path`);
  });
});
