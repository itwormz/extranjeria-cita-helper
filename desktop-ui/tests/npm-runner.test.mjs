import test from 'node:test';
import assert from 'node:assert/strict';
import {runNpm} from '../scripts/npm.mjs';
// Recent Node versions refuse to spawn `npm.cmd` directly on Windows (EINVAL), so every build script goes through this helper.
test('npm runs through the shared helper and reports its status', () => {
  const result = runNpm(['--version'], {stdio: 'pipe', encoding: 'utf8'});
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0);
  assert.match(String(result.stdout).trim(), /^\d+\.\d+\.\d+/);
});
test('a failing npm command is reported, not thrown', () => {
  const result = runNpm(['run', 'definitely-not-a-script', '--prefix', '.'], {stdio: 'pipe', encoding: 'utf8'});
  assert.notEqual(result.status, 0);
});
