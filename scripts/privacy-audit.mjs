import {readFileSync,readdirSync,mkdirSync,mkdtempSync} from 'node:fs';
import {resolve,join,basename} from 'node:path';
import {homedir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {sensitiveRule,validate} from './release-source.mjs';
export function privacyFindings(buffer,{binary=false}={}){
 const texts=binary?[buffer.toString('latin1'),buffer.toString('utf16le')]:[buffer.toString('utf8')];
 const findings=new Set();
 for(const text of texts){
  if(/\b(?:gh[pousr]_[A-Za-z0-9]{25,}|github_pat_[A-Za-z0-9_]{30,}|sk-[A-Za-z0-9_-]{20,}|AIza[A-Za-z0-9_-]{30,}|AKIA[A-Z0-9]{16})\b/.test(text))findings.add('credential');
  if(/-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/.test(text))findings.add('private key');
  if(/\beyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{15,}\b/.test(text))findings.add('login token');
  if(/[A-Za-z]:[\\/]+Users[\\/]+(?!Public\b|Default\b|Example\b|example\b|<)[A-Za-z0-9_.-]+[\\/]/.test(text))findings.add('local profile path');
  if(binary){if(text.includes(homedir())||text.includes(resolve('.'))||text.includes(basename(homedir())))findings.add('local build identity');}
  else {const rule=sensitiveRule(text);if(rule)findings.add(rule);}
 }
 return [...findings];
}
function auditFile(path){const binary=/\.(exe|dll|pdb|png|ico|jpg)$/i.test(path);const findings=privacyFindings(readFileSync(path),{binary});if(findings.length)throw Error(`Privacy review required: ${basename(path)} (${findings.join(', ')}; values redacted)`);}
function walk(path){for(const name of readdirSync(path,{withFileTypes:true})){const p=join(path,name.name);if(name.isDirectory())walk(p);else if(/\.(exe|dll|pdb|js|json|rs|md|toml|txt|svg|lock)$/i.test(name.name))auditFile(p);}}
export function auditArchive(path){
 const root=resolve('.qa');mkdirSync(root,{recursive:true});const target=mkdtempSync(join(root,'privacy-unpack-'));
 const seven=process.env.SEVEN_ZIP||(process.platform==='win32'?'C:/Program Files/7-Zip/7z.exe':'7z');
 execFileSync(seven,['x',resolve(path),`-o${target}`,'-y'],{stdio:'pipe'});walk(target);
 console.log(`Extracted ${basename(path)}: privacy scan passed`);
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 try{
  const args=process.argv.slice(2);if(!args.length){const result=validate(resolve('.'));for(const file of result.files)auditFile(file);console.log(`Reviewed source: ${result.files.length} files`);}
  for(let i=0;i<args.length;i++){
   if(args[i]==='--file')auditFile(args[++i]);
   else if(args[i]==='--archive')auditArchive(args[++i]);
   else if(args[i]==='--history'){
    const commits=execFileSync('git',['rev-list','--all']).toString().trim().split('\n');let total=0;
    for(const commit of commits){const paths=execFileSync('git',['ls-tree','-r','--name-only',commit]).toString().trim().split('\n');for(const path of paths){const data=execFileSync('git',['show',`${commit}:${path}`],{maxBuffer:20*1024*1024});const found=privacyFindings(data,{binary:/\.(png|ico|jpg)$/i.test(path)});if(found.length)throw Error(`History privacy review required: ${path} (${found.join(', ')}; values redacted)`);total++;}}
    console.log(`Git history: ${commits.length} commits, ${total} files passed`);
   }else throw Error(`Unknown audit option ${args[i]}`);
  }
  console.log('Privacy audit passed');
 }catch(e){console.error(e.message);process.exitCode=1;}
}
