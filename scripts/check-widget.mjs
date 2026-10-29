import { chromium } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { setTimeout as pause } from 'node:timers/promises';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
const launch=args=>spawn(process.argv[2],args,{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9237 --remote-debugging-address=127.0.0.1'},stdio:'ignore'});
let child=launch([]);
let browser,startupIpc,originalStartup,original,restoreIpc;
try{
  for(let i=0;i<60;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9237');break;}catch{await pause(300);}}
  if(!browser)throw new Error('Cannot connect to WebView');
  let page;
  for(let i=0;i<50;i++){page=browser.contexts()[0].pages().find(p=>/tauri.localhost|127.0.0.1:1420/.test(p.url()));if(page)break;await pause(300);}
  if(!page)throw new Error('Application page missing');
  const rawIpc=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  const ipc=async(command,args={})=>{for(let attempt=0;;attempt++){try{return await rawIpc(command,args);}catch(error){if(attempt>=90||!((command==='save_settings'&&String(error).includes('正在查询该平台'))||(command==='refresh_provider'&&String(error).includes('正在刷新'))))throw error;await pause(500);}}};
  original=await ipc('get_state');restoreIpc=ipc;
  if(original.settings.codexPath.toLowerCase()==='c:\\windows\\system32\\where.exe'){original.settings.codexPath='';await ipc('save_settings',{settings:original.settings});console.log('Recovered Codex auto-detection after an interrupted test');}
  startupIpc=ipc;originalStartup=await ipc('autostart_enabled');
  if(original.settings.desktopMode)await ipc('set_display_mode',{desktop:false});
  await page.getByRole('heading',{name:'额度，一眼看清。'}).waitFor();
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.getByRole('button',{name:'打开设置'}).click();
  const startup=page.getByRole('checkbox',{name:/开机自启/});
  await startup.waitFor();await page.waitForFunction(()=>!Array.from(document.querySelectorAll('.switch-row')).find(el=>el.textContent.includes('开机自启'))?.querySelector('input')?.disabled);
  await startup.click();
  await page.waitForFunction(async expected=>(await window.__TAURI_INTERNALS__.invoke('autostart_enabled'))===expected,!originalStartup);
  if(!originalStartup){
    const run=execFileSync('powershell.exe',['-NoProfile','-Command',"(Get-ItemProperty -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'Token++').'Token++'"],{encoding:'utf8'}).trim();
    assert.ok(run.startsWith('"')&&run.includes('--autostart'),'Windows startup entry must quote the executable path and pass startup flag');
  }
  await startup.click();
  await page.waitForFunction(async expected=>(await window.__TAURI_INTERNALS__.invoke('autostart_enabled'))===expected,originalStartup);
  await page.getByRole('button',{name:'返回额度一览'}).click();
  await ipc('save_settings',{settings:{...(await ipc('get_state')).settings,liveBackground:false}});
  await ipc('set_display_mode',{desktop:true});
  await page.locator('.desktop-widget').waitFor();await pause(1500);
  assert.equal(await page.locator('.widget-brand').textContent(),'Token++');
  assert.equal(await page.locator('.widget-platform-icon').evaluateAll(images=>images.every(img=>img.complete&&img.naturalWidth>0)),true,'local platform icons must load');
  const quotaLabels=await page.locator('.widget-platform.antigravity .widget-quota-line>span').allTextContents();
  if(quotaLabels.some(label=>/5\s*小时/.test(label))&&quotaLabels.some(label=>/每周/.test(label))){assert.ok(quotaLabels.findIndex(label=>/5\s*小时/.test(label))<quotaLabels.findIndex(label=>/每周/.test(label)));}
  const background=await ipc('desktop_background',{knownRevision:''});
  assert.equal(background.attached,true,'native window must belong to desktop host');
  console.log('Desktop host attached; wallpaper sample available:',!!background.dataUrl);
  const toolsReachable=original.configured.antigravity&&await ipc('list_antigravity_accounts').then(()=>true,()=>false);
  if(original.configured.antigravity&&!toolsReachable)console.log('Tools management service unavailable; account-switch network checks skipped');
  if(original.configured.deepseek&&original.configured.huawei){
    await page.waitForFunction(()=>document.querySelectorAll('.widget-track').length>=2,{},{timeout:45000});
  }
  await mkdir('.qa',{recursive:true});
  if(process.env.TOKEN_SKIP_SCREENSHOTS!=='1')await page.screenshot({path:'.qa/widget-native-frosted.png'});
  const state=await ipc('get_state');
  assert.equal(state.settings.desktopMode,true);
  const placement=await ipc('desktop_placement');
  assert.equal('dataUrl' in placement,false,'drag updates must carry coordinates only');
  if(background.dataUrl){
    const image=await page.locator('.widget-backdrop > .wallpaper-sample').evaluate(el=>el.style.backgroundImage);
    await ipc('plugin:event|emit',{event:'desktop-placement',payload:{...placement,x:137,y:83}});
    await page.waitForFunction(()=>Array.from(document.querySelectorAll('.wallpaper-sample')).every(el=>getComputedStyle(el).left==='-137px'));
    assert.equal(await page.locator('.widget-backdrop > .wallpaper-sample').evaluate(el=>el.style.backgroundImage),image);
    await ipc('plugin:event|emit',{event:'desktop-placement',payload:placement});
  }
  assert.equal(state.configured.deepseek,original.configured.deepseek);
  assert.equal(state.configured.huawei,original.configured.huawei);
  if(toolsReachable){
    const selector=page.getByRole('combobox',{name:'Antigravity 查看账号'});
    await selector.waitFor();
    await page.waitForFunction(()=>!document.querySelector('[aria-label="Antigravity 查看账号"]')?.disabled,{},{timeout:90000});
    await page.evaluate(async()=>{
      window.__accountRefreshCalls=[];
      const handler=window.__TAURI_INTERNALS__.transformCallback(event=>window.__accountRefreshCalls.push(event.payload));
      await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'refresh-started',target:{kind:'Any'},handler});
    });
    await selector.selectOption('__all__');
    await page.waitForFunction(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('get_state');return s.settings.antigravityAccount==='__all__'&&s.snapshots.some(p=>p.provider==='antigravity'&&p.view?.account==='__all__'&&p.metrics.length>0);},{},{timeout:90000});
    await page.waitForFunction(()=>!document.querySelector('[aria-label="Antigravity 查看账号"]')?.disabled,{},{timeout:90000});
    assert.equal(await page.locator('.widget-platform.antigravity time').count(),0,'aggregate must not invent a shared reset');
    assert.deepEqual((await ipc('get_state')).settings.antigravityGroups,state.settings.antigravityGroups,'account switch must preserve the global display preference');
    assert.equal(await page.getByText('设置已保存',{exact:true}).count(),0,'option changes must save quietly');
    const accountRefreshes=await page.evaluate(()=>window.__accountRefreshCalls);
    assert.ok(accountRefreshes.includes('antigravity'));
    assert.ok(accountRefreshes.every(id=>id==='antigravity'),'account switch must not trigger unrelated platforms');
    await ipc('save_settings',{settings:state.settings});
    const restored=await ipc('refresh_provider',{provider:'antigravity'});
    assert.ok(restored.metrics.length>0,'original single account must recover after combined view');
    await assert.rejects(()=>ipc('save_settings',{settings:{...state.settings,antigravityUrl:'http://example.com:8045'}}),/https/);
    try{
      await ipc('save_settings',{settings:{...state.settings,antigravityUrl:'https://tools.example.com',antigravityAccount:''}});
      assert.equal((await ipc('get_state')).configured.antigravity,false,'old credential must not authorize the new destination');
      await assert.rejects(()=>ipc('list_antigravity_accounts'),/服务地址已变化/);
    }finally{await ipc('save_settings',{settings:state.settings});}
    assert.equal((await ipc('get_state')).configured.antigravity,true,'restoring original destination restores its bound credential');
    await ipc('refresh_provider',{provider:'antigravity'});
    await ipc('set_display_mode',{desktop:false});
    await page.getByRole('button',{name:'打开设置'}).click();
    await page.getByRole('button',{name:'Antigravity',exact:true}).click();
    await page.getByLabel('显示项',{exact:true}).selectOption('claude_gpt');
    await page.getByLabel('显示的账号').selectOption('__all__');
    assert.equal(await page.getByLabel('显示项',{exact:true}).inputValue(),'claude_gpt');
    await page.getByRole('button',{name:'返回额度一览'}).click();
    await ipc('set_display_mode',{desktop:true});
  }
  await ipc('save_settings',{settings:{...state.settings,glass:'liquid',glassTone:'dark',fontScale:120}});
  await page.locator('.desktop-widget.liquid.dark').waitFor();await pause(800);
  assert.equal(await page.locator('.widget-grip svg').count(),original.settings.positionLocked?1:0);
  assert.equal(await page.locator('.desktop-widget').evaluate(el=>getComputedStyle(el,'::after').content),'none');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  if(process.env.TOKEN_SKIP_SCREENSHOTS!=='1')await page.screenshot({path:'.qa/widget-native-liquid-dark.png'});
  await ipc('save_settings',{settings:{...state.settings,compact:false}});await pause(700);
  await page.locator('.desktop-widget.standard').waitFor();
  await ipc('set_display_mode',{desktop:false});
  await page.getByRole('button',{name:'进入桌面模式'}).waitFor();
  if(process.env.TOKEN_SKIP_SCREENSHOTS!=='1')await page.screenshot({path:`.qa/panel-${original.version}.png`});
  const restored=await ipc('get_state');
  assert.equal(restored.settings.antigravityAccount,original.settings.antigravityAccount);
  assert.deepEqual(restored.settings.antigravityGroups,original.settings.antigravityGroups);
  await ipc('save_settings',{settings:{...restored.settings,compact:original.settings.compact,glass:original.settings.glass,glassTone:original.settings.glassTone,fontScale:original.settings.fontScale}});
  try{
    await ipc('save_settings',{settings:{...(await ipc('get_state')).settings,codexPath:'C:\\Windows\\System32\\where.exe'}});
    // Changing executable deliberately invalidates account-dependent cache.
    const beforeFailure=(await ipc('get_state')).snapshots.find(s=>s.provider==='codex');
    let failed;
    for(let i=0;i<40;i++){
      try{failed=await ipc('refresh_provider',{provider:'codex'});break;}
      catch(e){if(!String(e).includes('正在刷新'))throw e;await pause(1000);}
    }
    assert.ok(failed,'refresh must finish after query process exits');
    assert.equal(failed.status,beforeFailure.updatedAt?'stale':'error');
    assert.deepEqual(failed.metrics,beforeFailure.metrics,'failed query must preserve cached allowance');
    assert.equal(failed.updatedAt,beforeFailure.updatedAt);
    await ipc('get_state');
    await page.evaluate(async()=>{
      window.__retryCalls=[];
      const handler=window.__TAURI_INTERNALS__.transformCallback(event=>window.__retryCalls.push(event.payload));
      await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'refresh-started',target:{kind:'Any'},handler});
    });
    const retry=page.getByRole('button',{name:'重试 Codex',exact:true});
    await retry.click();
    await page.waitForFunction(()=>window.__retryCalls?.length===1);
    await page.waitForFunction(()=>!document.querySelector('[aria-label="重试 Codex"]')?.disabled);
    assert.deepEqual(await page.evaluate(()=>window.__retryCalls),['codex'],'retry must target only the failed provider');
    await ipc('set_display_mode',{desktop:true});
    const widgetRetry=page.getByRole('button',{name:'重试 Codex',exact:true});
    await widgetRetry.click();
    await page.waitForFunction(()=>window.__retryCalls?.length===2);
    await page.waitForFunction(()=>!document.querySelector('[aria-label="重试 Codex"]')?.disabled);
    assert.deepEqual(await page.evaluate(()=>window.__retryCalls),['codex','codex']);
    if(process.env.TOKEN_SKIP_SCREENSHOTS!=='1')await page.screenshot({path:'.qa/widget-retry.png'});
    await ipc('set_display_mode',{desktop:false});
  }finally{
    await ipc('save_settings',{settings:{...(await ipc('get_state')).settings,codexPath:original.settings.codexPath}});
  }
  const recovered=await ipc('refresh_provider',{provider:'codex'});
  assert.equal(recovered.status,'ready','refresh can recover without restarting widget');
  assert.deepEqual(errors,[]);
  for(let i=0;i<10;i++){
    await ipc('set_display_mode',{desktop:true});await pause(180);
    const frame=await ipc('desktop_frame');
    assert.equal(frame.attached,true);assert.equal(frame.frameClean,true,'desktop must keep native frame bits cleared after transitions');
    assert.equal(frame.nativeRegion,frame.win10Compatibility,'only Win10 uses native region fallback');
    await ipc('set_display_mode',{desktop:false});
    const panel=await ipc('desktop_frame');assert.equal(panel.attached,false);assert.equal(panel.nativeRegion,false,'panel must not inherit widget clipping');
  }
  await ipc('set_display_mode',{desktop:true});
  await ipc('set_display_mode',{desktop:false});
  await browser.close();child.kill();await pause(700);
  child=launch(['--autostart']);browser=undefined;
  for(let i=0;i<60;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9237');break;}catch{await pause(300);}}
  assert.ok(browser);page=browser.contexts()[0].pages()[0];await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  assert.equal(await ipc('plugin:window|is_visible',{label:'main'}),false,'autostart of full-panel mode must stay in tray');
  assert.equal((await ipc('get_state')).settings.desktopMode,false);
  await ipc('set_display_mode',{desktop:true});
  assert.equal(await ipc('plugin:window|is_visible',{label:'main'}),true);
  console.log('Widget check passed: startup UI/system registration/rollback and tray-mode launch, desktop host, live data, glass, single-provider retry and recovery, settings preserved.');
}finally{if(browser){if(restoreIpc&&original){try{await restoreIpc('save_settings',{settings:original.settings});}catch{}}if(startupIpc&&originalStartup!==undefined){try{await startupIpc('set_autostart',{enabled:originalStartup});}catch{}}await browser.close();child.kill();}}
