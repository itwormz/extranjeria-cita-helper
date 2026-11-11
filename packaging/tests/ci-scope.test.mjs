import test from 'node:test';import assert from 'node:assert/strict';
import {classifyPaths} from '../ci-scope.mjs';
test('only maintained documentation skips compilation',()=>{
 assert.equal(classifyPaths(['README.ru.md','docs/BUILD.md','docs/images/workflow-en-dark.svg']),'docs');
 for(const files of [[],['README.md','Cargo.toml'],['.github/workflows/build.yml'],['desktop-ui/src/messages.json'],['.github/release-version'],['docs/../../Cargo.toml']])assert.equal(classifyPaths(files),'full');
 assert.equal(classifyPaths(['README.md'],false),'full');
});
