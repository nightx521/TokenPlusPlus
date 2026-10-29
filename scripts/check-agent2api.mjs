import {chromium} from '@playwright/test';
import {spawn} from 'node:child_process';import {createServer} from 'node:http';import {setTimeout as pause} from 'node:timers/promises';import assert from 'node:assert/strict';
const key='TokenPlusPlus-isolated-fixture';let mode='ready',active=0,peak=0;const requested=[];
const server=createServer(async(req,res)=>{
 const url=new URL(req.url,'http://localhost');requested.push(url.pathname+url.search);
 if(req.headers.authorization!==`Bearer ${key}`||mode==='unauthorized'){res.writeHead(401).end('{}');return;}
 if(mode==='limited'){res.writeHead(429,{'Retry-After':'1'}).end('{}');return;}
 res.setHeader('Content-Type','application/json');let data;
 if(url.pathname==='/api/accounts')data={accounts:Array.from({length:6},(_,i)=>({id:`fixture-${i}`,name:`示例账号 ${i}`,provider:'qoder',enabled:i!==5,accessToken:'fixture-must-not-reach-ui'}))};
 else if(url.pathname==='/api/models/manage')data={models:[{id:'glm-4.6',provider:'qoder',enabled:true},{id:'claude-sonnet',provider:'qoder',enabled:true}],mappings:[{alias:'synthetic-alias',target:'claude-sonnet',provider:'qoder'}]};
 else if(url.pathname==='/api/accounts/usage/snapshot')data={at:Date.now(),results:[]};
 else if(url.pathname==='/api/accounts/usage'){
  active++;peak=Math.max(active,peak);await pause(mode==='slow'?700:60);active--;const id=url.searchParams.get('id');
  data={results:[{id,error:mode==='partial'&&id==='fixture-1'?'fixture-must-not-reach-ui':null,usage:{available:'123.45',unit:'CNY',wallets:[{type:'usd',displayName:'美元余额',balance:'7.50',unit:'USD'}],raw:{token:'fixture-must-not-reach-ui'}}}]};
 }else{res.writeHead(404).end('{}');return;}res.end(JSON.stringify({success:true,data}));
});await new Promise(r=>server.listen(0,'127.0.0.1',r));const address=`http://127.0.0.1:${server.address().port}`;
const child=spawn(process.argv[2],['--panel'],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9237 --remote-debugging-address=127.0.0.1'},stdio:'ignore'});
let browser,original,ipc,saved=false;
try{
 for(let i=0;i<60;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9237');break;}catch{await pause(300);}}assert.ok(browser);
 const page=browser.contexts()[0].pages()[0];await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
 const raw=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
 ipc=async(command,args={})=>{for(let i=0;;i++){try{return await raw(command,args);}catch(e){if(i>60||!/正在(查询|刷新)/.test(String(e)))throw e;await pause(500);}}};
 original=await ipc('get_state');assert.equal(original.configured.agent2api,false,'Never overwrite an existing management credential');
 await ipc('save_settings',{settings:{...original.settings,desktopMode:false,agent2apiUrl:address,agent2apiSelection:{version:1,accounts:null,models:{},metrics:{}}}});
 await page.getByRole('button',{name:'打开设置'}).click();await page.getByRole('button',{name:'Agent2API',exact:true}).click();await page.getByLabel('管理访问令牌').fill(key);saved=true;await page.getByRole('button',{name:'测试连接并读取额度'}).click();
 await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('get_state');return s.snapshots.some(p=>p.provider==='agent2api'&&p.status==='ready');},{},{timeout:45000});
 await page.waitForFunction(()=>!document.querySelector('.agent-settings .primary-button')?.disabled);
 await page.getByRole('button',{name:'返回额度一览'}).click();await page.locator('.provider.agent2api .agent-account').first().waitFor();
 const state=await ipc('get_state');assert.equal(state.snapshots.find(s=>s.provider==='agent2api').gateway.accounts.length,6);assert.equal(JSON.stringify(state.snapshots.find(s=>s.provider==='agent2api')).includes('fixture-must-not-reach-ui'),false);assert.ok(peak<=4);assert.ok(peak>=2);assert.ok(!requested.some(p=>p.includes('id=fixture-5')));
 let settings={...state.settings,agent2apiSelection:{version:1,accounts:['fixture-0'],models:{'fixture-0':['glm-4.6']},metrics:{'fixture-0':['available']}}};await ipc('save_settings',{settings});await page.waitForFunction(()=>document.querySelectorAll('.provider.agent2api .agent-account').length===1);assert.equal(await page.locator('.provider.agent2api .agent-model').count(),1);
 mode='slow';const refresh=ipc('refresh_provider',{provider:'agent2api'});await pause(200);await page.locator('.provider.agent2api').getByText('¥ 123.45',{exact:true}).waitFor();await refresh;
 await ipc('set_display_mode',{desktop:true});await page.locator('.widget-platform.agent2api .agent-account').waitFor();assert.equal(await page.locator('.widget-platform.agent2api .agent-account').count(),1);await page.locator('.widget-platform.agent2api').getByText('¥ 123.45',{exact:true}).waitFor();
  const detailOpened=page.context().waitForEvent('page');await ipc('open_agent2api_details',{account:'fixture-0'});const detail=await detailOpened;await detail.locator('.agent-details-window .agent-reading').first().waitFor();assert.equal(await detail.locator('.agent-details-list .agent-reading').count(),2,'details includes the complete list');await detail.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close',{label:'agent2api-details'})).catch(error=>{if(!/closed|destroyed|detached/i.test(String(error)))throw error;});
 mode='unauthorized';const failed=await ipc('refresh_provider',{provider:'agent2api'});assert.equal(failed.status,'stale');assert.ok(failed.gateway.accounts[0].metrics.length>0);const after401=requested.length;await ipc('refresh_provider',{provider:'agent2api'});assert.equal(requested.length,after401,'401 circuit breaker must prevent further requests');
 mode='ready';await ipc('save_credentials',{provider:'agent2api',key,secret:''});const recovered=await ipc('refresh_provider',{provider:'agent2api'});assert.equal(recovered.status,'ready');
 console.log('Agent2API end-to-end passed: connection UI, sanitized payload, four-way concurrency, hidden accounts, synced selections, old values, 401 stop and credential recovery.');
}finally{
 if(ipc&&original){try{if(saved)await ipc('remove_credentials',{provider:'agent2api'});await ipc('save_settings',{settings:original.settings});}catch(e){console.error('Fixture restore failed',String(e));}}
 await browser?.close();child.kill();await new Promise(r=>server.close(r));
}
