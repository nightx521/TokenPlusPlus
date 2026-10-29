import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { setTimeout as pause } from 'node:timers/promises';
import assert from 'node:assert/strict';
const key='TokenPlusPlus-isolated-fixture';
const now=Math.floor(Date.now()/1000);
const quota={last_updated:now,quota_groups:['Gemini','Claude'].map((name,i)=>({display_name:name,buckets:['5h','weekly'].map((window,j)=>({bucket_id:`${name}-${window}`,window,remaining_fraction:0.8-i*.1-j*.2,reset_time:new Date((now+(j+1)*86400)*1000).toISOString(),observed_at:now*1000}))}))};
let mode='ready',freshCalls=0,unexpected=0;
const server=createServer((request,response)=>{
  if(mode==='limited'){response.writeHead(429,{'Retry-After':'1'}).end();return;}
  if(request.headers.authorization!==`Bearer ${key}`||mode==='unauthorized'){response.writeHead(401).end();return;}
  response.setHeader('Content-Type','application/json');
  if(request.method==='GET'&&request.url==='/api/accounts')response.end(JSON.stringify({accounts:[{id:'mock-a',name:'测试账号',quota,token:{refresh_token:'fixture-must-not-reach-ui'}}]}));
  else if(request.method==='GET'&&request.url==='/api/accounts/mock-a/quota'){freshCalls++;response.end(JSON.stringify(quota));}
  else{unexpected++;response.writeHead(404).end('{}');}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const port=server.address().port;
const child=spawn(process.argv[2],[],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9237 --remote-debugging-address=127.0.0.1'},stdio:'ignore'});
let browser,original,ipc,saved=false;
async function retryBusy(action){for(let i=0;i<40;i++){try{return await action();}catch(e){if(!/正在(刷新|查询)/.test(String(e)))throw e;await pause(500);}}throw Error('Refresh did not finish');}
try{
  for(let i=0;i<60;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9237');break;}catch{await pause(300);}}
  assert.ok(browser);
  const page=browser.contexts()[0].pages()[0];await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  ipc=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  original=await ipc('get_state');
  if(original.configured.antigravity&&process.argv.includes('--cleanup-fixture')){
    await retryBusy(()=>ipc('save_settings',{settings:{...original.settings,antigravityPort:port,antigravityAccount:'mock-a',antigravityGroups:[]}}));
    const probe=await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}));
    assert.equal(probe.status,'ready','Only remove a credential authenticated by this isolated fixture');
    await retryBusy(()=>ipc('remove_credentials',{provider:'antigravity'}));
    await retryBusy(()=>ipc('save_settings',{settings:original.settings}));
    original.configured.antigravity=false;
  }
  assert.equal(original.configured.antigravity,false,'Never overwrite an existing Tools credential');
  await ipc('set_display_mode',{desktop:false});
  await page.getByRole('button',{name:'打开设置'}).click();
  await page.getByRole('button',{name:'Antigravity',exact:true}).click();
  await page.getByLabel('Tools 服务地址').fill(`localhost:${port}`);
  await page.getByLabel('Tools 管理密钥').fill(key);
  saved=true;
  await page.getByRole('button',{name:'保存连接并读取额度'}).click();
  await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('get_state');return s.settings.antigravityAccount==='mock-a'&&s.snapshots.some(s=>s.provider==='antigravity'&&s.status==='ready');},{},{timeout:45000});
  await expect(page.getByLabel('Tools 管理密钥')).toHaveValue('');
  await expect(page.getByLabel('显示的账号')).toHaveValue('mock-a');
  await page.screenshot({path:'.qa/antigravity-settings-fixture.png'});
  const accounts=await ipc('list_antigravity_accounts');assert.equal(accounts.length,1);assert.deepEqual(accounts[0].groups,['Gemini','Claude']);
  assert.equal(JSON.stringify(accounts).includes('fixture-must-not-reach-ui'),false);
  const snapshot=await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}));assert.equal(snapshot.status,'ready');assert.equal(snapshot.metrics.length,4);assert.equal(snapshot.updatedAt,now);assert.ok(freshCalls>0);
  await ipc('set_display_mode',{desktop:true});await page.locator('.widget-platform.antigravity').waitFor();assert.equal(await page.locator('.widget-platform.antigravity .widget-track').count(),4);
  await page.screenshot({path:'.qa/widget-antigravity-fixture.png'});
  mode='unauthorized';
  const failed=await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}));assert.equal(failed.status,'stale');assert.deepEqual(failed.metrics,snapshot.metrics);assert.equal(failed.updatedAt,now);
  mode='ready';assert.equal((await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}))).status,'ready');
  mode='limited';assert.equal((await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}))).status,'stale');
  await assert.rejects(()=>ipc('refresh_provider',{provider:'antigravity'}),/限流/);
  await pause(1200);mode='ready';
  await retryBusy(async()=>ipc('save_settings',{settings:{...(await ipc('get_state')).settings,antigravityGroups:['Gemini']}}));
  assert.equal((await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}))).metrics.length,2);
  await new Promise(resolve=>server.close(resolve));
  const stopped=await retryBusy(()=>ipc('refresh_provider',{provider:'antigravity'}));assert.equal(stopped.status,'stale');assert.equal(stopped.metrics.length,2);
  assert.equal(unexpected,0);
  assert.equal(JSON.stringify(await ipc('get_state')).includes(key),false);
  console.log('Antigravity native fixture passed: authenticated loopback requests, sanitized account choices, both windows, shared group selection, source time, 401/cache/recovery, service-off cache, no inference requests.');
}finally{
  if(ipc&&original){
    try{if(saved)await retryBusy(()=>ipc('remove_credentials',{provider:'antigravity'}));await retryBusy(()=>ipc('save_settings',{settings:original.settings}));}finally{await browser?.close();child.kill();}
  }else{await browser?.close();child.kill();}
  if(server.listening)await new Promise(resolve=>server.close(resolve));
}
