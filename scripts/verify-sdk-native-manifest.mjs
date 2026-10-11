import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';

const [directory, version, target] = process.argv.slice(2);
assert(directory && version && target, 'usage: <extracted-directory> <version> <target>');
const manifest = JSON.parse(await readFile(path.join(directory, 'binding.json'), 'utf8'));
assert.equal(manifest.version, version);
assert.equal(manifest.target, target);
assert.equal(manifest.file, 'operon-sdk-native.node');
assert.equal(manifest.napi, 8);
assert.equal(manifest.sha256, createHash('sha256').update(await readFile(path.join(directory, manifest.file))).digest('hex'));
console.log(`PASS native binding manifest: ${target}@${version}`);
