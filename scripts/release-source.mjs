import { readdirSync, readFileSync, lstatSync, mkdirSync, copyFileSync, writeFileSync, mkdtempSync, existsSync } from 'node:fs';
import { resolve, relative, join, extname } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const rootFiles = ['.gitignore', 'README.md', 'LICENSE', 'CONTRIBUTING.md', 'SECURITY.md', 'THIRD_PARTY_NOTICES.md', 'CHANGELOG.md', 'package.json', 'package-lock.json', 'index.html', 'tsconfig.json', 'tsconfig.node.json', 'vite.config.ts', 'playwright.config.ts'];
export const sourceDirs = ['src', 'public', 'assets/branding', 'tests', '.github', 'docs'];
export const rustFiles = ['native-glass/Cargo.toml', 'native-glass/Cargo.lock', 'native-glass/README.md', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/build.rs', 'src-tauri/tests/agent2api_live.rs', 'src-tauri/tauri.conf.json', 'src-tauri/icons/icon.ico', 'src-tauri/icons/icon.png'];
export const rustDirs = ['native-glass/src', 'src-tauri/src', 'src-tauri/capabilities'];
export const scriptFiles = ['scripts/check-agent2api.mjs','scripts/package-private.mjs','scripts/privacy-audit.mjs','scripts/privacy-audit.test.mjs','scripts/check-live.mjs', 'scripts/move-native-test.ps1', 'scripts/release-source.mjs', 'scripts/release-source.test.mjs', 'scripts/check-widget.mjs', 'scripts/check-desktop.mjs', 'scripts/check-antigravity.mjs'];
const binaryExtensions = new Set(['.png', '.ico', '.jpg', '.jpeg', '.webp']);
const textExtensions = new Set(['.md', '.txt', '.json', '.toml', '.lock', '.rs', '.ts', '.tsx', '.js', '.mjs', '.css', '.html', '.svg', '.yml', '.yaml', '.ps1']);
const rules = [
  ['private key', /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/],
  ['bearer credential', /Bearer\s+[A-Za-z0-9_\-.]{24,}/i],
  ['API credential', /\b(?:sk-[A-Za-z0-9_-]{20,}|AIza[A-Za-z0-9_-]{30,}|AKIA[A-Z0-9]{16})\b/],
  ['local profile path', /[A-Za-z]:[\\/]+Users[\\/]+(?!Public\b|Default\b|Example\b|example\b|<)[A-Za-z0-9_.-]+[\\/]/],
  ['account email', /\b[A-Z0-9._%+-]+@(?!example\.(?:com|org)\b)[A-Z0-9.-]+\.[A-Z]{2,}\b/i],
];
export function sensitiveRule(text) { return rules.find(([, pattern]) => pattern.test(text))?.[0]; }

export function collectFiles(root) {
  const files = [];
  const add = path => {
    let ancestor = resolve(root);
    for (const part of path.split('/')) {
      ancestor = join(ancestor, part);
      if (lstatSync(ancestor).isSymbolicLink()) throw new Error(`Symbolic link is not allowed: ${path}`);
    }
    const absolute = resolve(root, path);
    const info = lstatSync(absolute);
    if (info.isSymbolicLink()) throw new Error(`Symbolic link is not allowed: ${path}`);
    if (info.isDirectory()) {
      for (const name of readdirSync(absolute).sort()) add(`${path}/${name}`);
    } else {
      const name = path.split('/').pop();
      if (/(?:^\.|\.env|\.pem$|\.key$|^state\.)/i.test(name) && path !== '.gitignore') throw new Error(`Private file is not allowed: ${path}`);
      if (!textExtensions.has(extname(path)) && !binaryExtensions.has(extname(path)) && !['.gitignore', 'LICENSE', 'LOBE-ICONS-LICENSE'].includes(name)) throw new Error(`Unreviewed file type: ${path}`);
      files.push(path);
    }
  };
  for (const file of [...rootFiles, ...rustFiles, ...scriptFiles]) if (existsSync(join(root, file))) add(file);
  for (const dir of [...sourceDirs, ...rustDirs]) if (existsSync(join(root, dir))) add(dir);
  return files.sort();
}

export function validate(root) {
  const files = collectFiles(root);
  for (const required of [...rootFiles.filter(p => p !== 'tsconfig.node.json'), ...rustFiles, ...scriptFiles]) {
    if (!files.includes(required)) throw new Error(`Required source file missing: ${required}`);
  }
  for (const path of files) {
    if (binaryExtensions.has(extname(path))) continue;
    const rule = sensitiveRule(readFileSync(join(root, path), 'utf8'));
    if (rule) throw new Error(`Review required: ${path} (${rule}; content not printed)`);
  }
  const pkg = JSON.parse(readFileSync(join(root, 'package.json')));
  const lock = JSON.parse(readFileSync(join(root, 'package-lock.json')));
  const tauri = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json')));
  const cargo = readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8').match(/\[package\][\s\S]*?\bversion\s*=\s*"([^"]+)"/)?.[1];
  const cargoLock = readFileSync(join(root, 'src-tauri/Cargo.lock'), 'utf8').match(/name = "token-plusplus"\r?\nversion = "([^"]+)"/)?.[1];
  const frontend = readFileSync(join(root, 'src/types.ts'), 'utf8').match(/version: '([^']+)'/)?.[1];
  if (!/^0\.\d+\.\d+$|^[1-9]\d*\.\d+\.\d+$/.test(pkg.version) || [lock.version, lock.packages[''].version, tauri.version, cargo, cargoLock, frontend].some(v => v !== pkg.version)) throw new Error('Application version mismatch');
  if (!readFileSync(join(root, 'LICENSE'), 'utf8').includes('Copyright (c) 2026 murainy')) throw new Error('License author mismatch');
  for (const path of ['docs/images/panel-demo.png', 'docs/images/widget-demo.png', 'public/brands/LOBE-ICONS-LICENSE']) if (!files.includes(path)) throw new Error(`Required asset missing: ${path}`);
  return { version: pkg.version, files };
}

export function exportSource(root) {
  const { version, files } = validate(root);
  const out = join(root, 'artifacts'); mkdirSync(out, { recursive: true });
  const stage = mkdtempSync(join(out, 'source-'));
  const source = join(stage, `TokenPlusPlus-${version}`); mkdirSync(source);
  const manifest = [];
  for (const path of files) {
    const target = join(source, path); mkdirSync(resolve(target, '..'), { recursive: true }); copyFileSync(join(root, path), target);
    manifest.push(`${createHash('sha256').update(readFileSync(target)).digest('hex')}  ${path}`);
  }
  writeFileSync(join(source, 'SOURCE-SHA256.txt'), `${manifest.join('\n')}\n`);
  const zip = join(out, `TokenPlusPlus-${version}-source.zip`);
  if (existsSync(zip)) throw new Error(`Output already exists; choose another output directory or archive it first: ${relative(root, zip)}`);
  execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Add-Type -AssemblyName System.IO.Compression.FileSystem; [IO.Compression.ZipFile]::CreateFromDirectory($env:TOKEN_SOURCE_STAGE, $env:TOKEN_SOURCE_ZIP)'], { env: { ...process.env, TOKEN_SOURCE_STAGE: stage, TOKEN_SOURCE_ZIP: zip }, stdio: 'pipe' });
  writeFileSync(`${zip}.sha256`, `${createHash('sha256').update(readFileSync(zip)).digest('hex')}  ${relative(out, zip)}\n`);
  return { version, count: files.length, zip };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
  try {
    const result = process.argv.includes('--export') ? exportSource(root) : validate(root);
    console.log(`Public source check passed: ${result.version}, ${result.count ?? result.files.length} files${result.zip ? `, ${result.zip}` : ''}`);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
