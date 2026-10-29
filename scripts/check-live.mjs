import {chromium} from '@playwright/test';
import {spawn,execFileSync} from 'node:child_process';
import {setTimeout as pause} from 'node:timers/promises';
import assert from 'node:assert/strict';
const child=spawn(process.argv[2],[],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9237 --remote-debugging-address=127.0.0.1'},stdio:['ignore','pipe','pipe']});
child.stderr.on('data',data=>process.stderr.write(data));child.on('exit',(code,signal)=>console.log('application exit',code,signal));
let browser,original;
let ipc;
try{
 for(let i=0;i<60;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9237');break;}catch{await pause(300);}}
 assert.ok(browser,'WebView not ready');const page=browser.contexts()[0].pages()[0];await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
 ipc=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
 original=(await ipc('get_state')).settings;
 await ipc('set_display_mode',{desktop:false});
 await ipc('save_settings',{settings:{...(await ipc('get_state')).settings,liveBackground:true}});
 await ipc('set_display_mode',{desktop:true});
 await page.locator('.desktop-widget.live-background').waitFor();
 await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('desktop_live_status');return s.ready&&!s.error;},{},{timeout:30000});
 await page.waitForFunction(()=>{const card=document.querySelector('.desktop-widget');return card&&innerHeight>=Math.ceil(card.getBoundingClientRect().height);},{},{timeout:10000});
 for(const enabled of [false,true]){
  await ipc('save_settings',{settings:{...(await ipc('get_state')).settings,liveBackground:enabled}});
  await pause(500);
  await page.waitForFunction(()=>{const card=document.querySelector('.desktop-widget');return card&&innerHeight>=Math.ceil(card.getBoundingClientRect().height);},{},{timeout:10000});
 }
 await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('desktop_live_status');return s.ready&&!s.error;},{},{timeout:30000});
 console.log('static/live switch preserves complete card height');
 const monitors=await ipc('plugin:window|available_monitors');console.log('monitors',monitors.map(m=>({scale:m.scaleFactor,position:m.position,size:m.size})));
 for(const m of Array.from({length:Number(process.env.TOKEN_GLASS_ROUNDS||10)},()=>monitors).flat()){
  execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','scripts/move-native-test.ps1','-TargetPid',String(child.pid),'-X',String(m.position.x+60),'-Y',String(m.position.y+80)],{stdio:'inherit'});
  await pause(1200);const layout=await page.evaluate(()=>({dpr:devicePixelRatio,width:innerWidth,card:document.querySelector('.desktop-widget').getBoundingClientRect().width}));
  console.log('frame',await ipc('desktop_frame'));assert.ok(Math.abs(layout.dpr-m.scaleFactor)<.01,`DPI mismatch: ${JSON.stringify(layout)}`);assert.ok(Math.abs(layout.width-layout.card)<2);
  const frame=await ipc('desktop_frame');assert.equal(frame.frameClean,true);assert.equal(frame.nativeRegion,true);assert.equal(frame.captureExcluded,false);
  const status=await ipc('desktop_live_status');assert.equal(status.error,null);assert.equal(status.backend,'native-compositor');assert.equal(status.capturing,false);assert.ok(status.sources>=1);console.log('live-layout',layout,'ready',status.ready);
 }
 if(monitors.length>1){const second=monitors.find(m=>m.position.x>0)||monitors[1];execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','scripts/move-native-test.ps1','-TargetPid',String(child.pid),'-X',String(second.position.x-150),'-Y',String(Math.max(...monitors.map(m=>m.position.y))+80)],{stdio:'inherit'});await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('desktop_live_status');return s.sources>=2&&!s.error;},{},{timeout:15000});console.log('two-monitor composition passed');}
 if(process.env.TOKEN_GLASS_PIXELS==='1'){execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','scripts/move-native-test.ps1','-TargetPid',String(child.pid),'-X','100','-Y','100'],{stdio:'pipe'});await pause(1000);assert.equal(await ipc('desktop_probe_pixels'),true,'final screen must contain GPU pattern beneath actual widget');console.log('final compositor pixel test passed');}
 await ipc('plugin:window|hide',{label:'main'});await pause(500);const hidden=await ipc('desktop_live_status');assert.equal(hidden.sources,0);
 await ipc('set_display_mode',{desktop:true});await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('desktop_live_status');return s.sources>0&&!s.error;},{},{timeout:15000});
 await ipc('set_display_mode',{desktop:false});assert.equal((await ipc('desktop_live_status')).active,false);
 console.log('Live backdrop native checks passed');
}finally{
 if(ipc&&original){try{await ipc('save_settings',{settings:{...original,desktopMode:false}});await ipc('set_display_mode',{desktop:original.desktopMode});}catch(e){console.error('Restore failed',String(e));}}
 if(browser)await browser.close();child.kill();
}
