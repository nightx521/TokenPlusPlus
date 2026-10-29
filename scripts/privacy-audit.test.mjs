import {test} from 'node:test';
import assert from 'node:assert/strict';
import {privacyFindings} from './privacy-audit.mjs';
test('privacy audit covers binary UTF-16 paths and tokens without logging values',()=>{
 const path='C:'+String.fromCharCode(92)+'Users'+String.fromCharCode(92)+'PrivateBuildUser'+String.fromCharCode(92)+'source';
 assert.ok(privacyFindings(Buffer.from(path,'utf16le'),{binary:true}).includes('local profile path'));
 const key='ghp_'+'q'.repeat(36);assert.ok(privacyFindings(Buffer.from(key)).includes('credential'));
 assert.deepEqual(privacyFindings(Buffer.from('fixture: unknown balance; https://example.com')),[]);
});
