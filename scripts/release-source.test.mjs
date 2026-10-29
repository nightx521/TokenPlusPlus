import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, copyFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collectFiles, sensitiveRule, validate } from './release-source.mjs';

test('source export excludes local history, account data, outputs and dependencies', () => {
  const root = mkdtempSync(join(tmpdir(), 'token-export-'));
  for (const dir of ['src', '.qa', 'release', 'artifacts', 'node_modules', 'src-tauri/target', 'docs']) mkdirSync(join(root, dir), { recursive: true });
  for (const path of ['README.md', 'src/valid.ts', 'HANDOFF.md', 'AGENTS.md', '.env', '.qa/accounts.json', 'release/old.exe', 'artifacts/state.json', 'node_modules/index.js', 'src-tauri/target/old.exe', 'state.json']) writeFileSync(join(root, path), 'example');
  assert.deepEqual(collectFiles(root), ['README.md', 'src/valid.ts']);
  writeFileSync(join(root, 'docs/state.json'), '{}');
  assert.throws(() => collectFiles(root), /Private file/);
});
test('sensitive scan reports category without including the secret', () => {
  assert.equal(sensitiveRule('Bearer ' + 'a'.repeat(32)), 'bearer credential');
  assert.equal(sensitiveRule('sk-' + 'x'.repeat(32)), 'API credential');
  assert.equal(sensitiveRule('contact@example.com'), undefined);
  assert.equal(sensitiveRule('user' + '@' + 'private-mail.net'), 'account email');
  assert.equal(sensitiveRule('query failed; balance unknown'), undefined);
});
test('symbolic links cannot pull external files into public source', t => {
  const root = mkdtempSync(join(tmpdir(), 'token-export-link-'));
  mkdirSync(join(root, 'src')); mkdirSync(join(root, 'external'));
  try { symlinkSync(join(root, 'external'), join(root, 'src', 'link'), 'junction'); }
  catch (e) { if (e.code === 'EPERM') return t.skip('Symlink privilege unavailable'); throw e; }
  assert.throws(() => collectFiles(root), /Symbolic link/);
});
test('a junction in an allowlisted path ancestor is also rejected', t => {
  const root = mkdtempSync(join(tmpdir(), 'token-export-parent-'));
  mkdirSync(join(root, 'outside/branding'), { recursive: true });
  try { symlinkSync(join(root, 'outside'), join(root, 'assets'), 'junction'); }
  catch (e) { if (e.code === 'EPERM') return t.skip('Symlink privilege unavailable'); throw e; }
  assert.throws(() => collectFiles(root), /Symbolic link/);
});
test('public source validation detects version drift and secrets in included files', () => {
  const project = fileURLToPath(new URL('..', import.meta.url));
  const root = mkdtempSync(join(tmpdir(), 'token-export-validate-'));
  for (const path of collectFiles(project)) {
    const target = join(root, path); mkdirSync(dirname(target), { recursive: true }); copyFileSync(join(project, path), target);
  }
  const pkg = JSON.parse(readFileSync(join(root, 'package.json')));
  assert.equal(validate(root).version, pkg.version);
  writeFileSync(join(root, 'package.json'), JSON.stringify({ ...pkg, version: '99.0.0' }));
  assert.throws(() => validate(root), /version mismatch/);
  writeFileSync(join(root, 'package.json'), JSON.stringify(pkg));
  writeFileSync(join(root, 'docs/accidental-secret.md'), 'Bearer ' + 'z'.repeat(32));
  assert.throws(() => validate(root), e => e.message.includes('bearer credential') && !e.message.includes('z'.repeat(32)));
});
