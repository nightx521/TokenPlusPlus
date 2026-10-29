// Remap local build paths before rustc embeds panic/source locations in the executable.
import {spawnSync,execFileSync} from 'node:child_process';
import {homedir} from 'node:os';
import {join,resolve} from 'node:path';
const root=resolve(import.meta.dirname,'..');
const cargo=process.env.CARGO_HOME||join(homedir(),'.cargo');
const flags=[process.env.RUSTFLAGS||'',...[root,cargo,homedir()].flatMap((p,i)=>[p,p.replaceAll('\\','/')].map(v=>`--remap-path-prefix=${v}=/build/${['app','cargo','user'][i]}`)),...(process.platform==='win32'?['-C','link-arg=/PDBALTPATH:token-plusplus.pdb']:[])].filter(Boolean);
// Encoded flags preserve paths containing spaces. Do not interpolate paths into a shell command.
const cli=join(root,'node_modules','@tauri-apps','cli','tauri.js');
const result=spawnSync(process.execPath,[cli,'build','--bundles','nsis','--','--locked'],{cwd:root,stdio:'inherit',env:{...process.env,RUSTFLAGS:undefined,CARGO_ENCODED_RUSTFLAGS:flags.join('\x1f')}});
if(result.error)throw result.error;process.exitCode=result.status??1;
