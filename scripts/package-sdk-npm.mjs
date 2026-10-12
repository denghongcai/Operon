import assert from 'node:assert/strict';
import { cp, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const [artifacts, output] = process.argv.slice(2);
assert(artifacts && output, 'usage: <accepted-native-artifacts> <new-package-directory>');
const root = path.resolve(import.meta.dirname, '..');
const sdk = path.join(root, 'packages/sdk-js');
const source = JSON.parse(await readFile(path.join(sdk, 'package.json'), 'utf8'));
const targets = ['x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu',
  'x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl',
  'x86_64-apple-darwin', 'aarch64-apple-darwin', 'x86_64-pc-windows-msvc'];
async function walk(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(file));
    else if (entry.isFile()) files.push(file);
  }
  return files;
}
const files = await walk(path.resolve(artifacts));
await mkdir(output); // Never overwrite an existing package directory.
await cp(path.join(sdk, 'dist'), path.join(output, 'dist'), { recursive: true });
await cp(path.join(sdk, 'README.md'), path.join(output, 'README.md'));
await cp(path.join(root, 'LICENSE'), path.join(output, 'LICENSE'));
await cp(path.join(root, 'vendor/portable-pty-0.9.0-operon/LICENSE'), path.join(output, 'LICENSE-portable-pty'));
for (const target of targets) {
  const name = `operon-sdk-native-v${source.version}-${target}.tar.gz`;
  const matches = files.filter(file => path.basename(file) === name);
  assert.equal(matches.length, 1, `missing/duplicate accepted binding: ${name}`);
  const entries = execFileSync('tar', ['-tzf', matches[0]], { encoding: 'utf8' }).trim().split('\n').sort();
  assert.deepEqual(entries, ['binding.json', 'operon-sdk-native.node']);
  const directory = path.join(output, 'native', target);
  await mkdir(directory, { recursive: true });
  for (const entry of entries) {
    await writeFile(path.join(directory, entry), execFileSync('tar', ['-xOf', matches[0], entry], { maxBuffer: 100 * 1024 * 1024 }));
  }
  const manifest = JSON.parse(await readFile(path.join(directory, 'binding.json'), 'utf8'));
  assert.equal(manifest.version, source.version);
  assert.equal(manifest.target, target);
  assert.equal(manifest.napi, 8);
  assert.equal(manifest.file, 'operon-sdk-native.node');
  assert.equal(manifest.sha256, createHash('sha256').update(await readFile(path.join(directory, manifest.file))).digest('hex'));
}
await writeFile(path.join(output, 'package.json'), JSON.stringify({
  name: '@cellworks/operon-sdk-js', version: source.version,
  description: 'Operon capability runtime TypeScript SDK', type: 'module',
  main: './dist/index.js', types: './dist/index.d.ts',
  exports: { '.': { types: './dist/index.d.ts', import: './dist/index.js' } },
  files: ['dist', 'native', 'LICENSE', 'LICENSE-portable-pty', 'README.md'],
  engines: { node: '>=22' }, license: 'MPL-2.0',
  repository: { type: 'git', url: 'git+https://github.com/denghongcai/Operon.git', directory: 'packages/sdk-js' },
  publishConfig: { access: 'public', registry: 'https://registry.npmjs.org/' },
  dependencies: source.dependencies,
}, null, 2) + '\n');
console.log(`Prepared @cellworks/operon-sdk-js@${source.version} with seven verified native targets`);
