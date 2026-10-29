import { test, expect } from '@playwright/test';

test('empty states, demo, details and credential settings', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto('http://127.0.0.1:1420');
  await expect(page.getByRole('heading', { name: '额度，一眼看清。' })).toBeVisible();
  await expect(page.locator('.empty-value')).toHaveCount(4);
  await page.getByRole('button', { name: '体验演示' }).click();
  await expect(page.getByText('以下为演示数据，用于体验界面')).toBeVisible();
  await expect(page.getByText('¥ 128.50', { exact: true })).toBeVisible();
  await page.locator('.provider.huawei').getByRole('button', { name: '查看详情' }).click();
  await expect(page.getByText('MaaS 体验代金券', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '退出演示', exact: true }).first().click();
  await expect(page.locator('.empty-value')).toHaveCount(4);
  await page.getByRole('button', { name: '打开设置' }).click();
  await page.getByRole('button', { name: '华为云', exact: true }).click();
  await expect(page.getByLabel('账号站点')).toHaveValue('cn');
  await expect(page.getByLabel('Access Key（AK）')).toHaveAttribute('type', 'password');
  await expect(page.getByLabel('Secret Key（SK）')).toHaveAttribute('type', 'password');
  await page.getByRole('button', { name: 'Codex', exact: true }).click();
  await expect(page.getByLabel('Codex 程序路径（可选）')).toBeVisible();
  await page.getByRole('button',{name:'Antigravity',exact:true}).click();
  await expect(page.getByLabel('Tools 服务地址')).toHaveValue('http://127.0.0.1:8045');
  await expect(page.getByLabel('Tools 管理密钥')).toHaveAttribute('type','password');
  await expect(page.getByLabel('Tools 额度刷新间隔（分钟）')).toHaveValue('5');
  await expect(page.getByRole('button',{name:'保存连接并读取额度'})).toBeVisible();
  expect(errors).toEqual([]);
});

test('narrow layout has no horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 380, height: 728 });
  await page.goto('http://127.0.0.1:1420');
  await page.getByRole('button', { name: '体验演示' }).click();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: '.qa/demo-narrow.png' });
  await page.setViewportSize({ width: 560, height: 1180 });
  await page.screenshot({ path: '.qa/panel-demo.png' });
});

test('desktop layout, materials, compact density and full panel restoration',async({page})=>{
  await page.goto('http://127.0.0.1:1420');
  await page.getByRole('button',{name:'体验演示'}).click();
  await page.getByRole('button',{name:'进入桌面模式'}).click();
  await expect(page.locator('.desktop-widget.compact.frosted')).toBeVisible();
  await expect(page.locator('.widget-platform')).toHaveCount(3);
  await expect(page.getByRole('progressbar')).toHaveCount(2);
  await expect(page.getByText('演示数据',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'桌面菜单'}).click();
  await page.getByRole('button',{name:'切换玻璃材质'}).click();
  await expect(page.locator('.desktop-widget.liquid')).toBeVisible();
  await page.getByRole('button',{name:'切换精简 / 标准'}).click();
  await expect(page.locator('.desktop-widget.standard')).toBeVisible();
  await page.getByRole('button',{name:'关闭菜单',exact:true}).click();
  await page.locator('.desktop-widget').screenshot({path:'.qa/widget-preview.png'});
  await page.getByRole('button',{name:'打开完整面板',exact:true}).click();
  await expect(page.getByRole('heading',{name:'额度，一眼看清。'})).toBeVisible();
});


test('Tools address draft and compact account selector', async ({page})=>{
  await page.goto('http://127.0.0.1:1420');
  await page.getByRole('button',{name:'体验演示'}).click();
  await page.getByRole('button',{name:'打开设置'}).click();
  await page.getByRole('checkbox',{name:'Antigravity',exact:true}).check();
  await page.getByRole('button',{name:'保存设置',exact:true}).click();
  await page.getByRole('button',{name:'Antigravity',exact:true}).click();
  const display=page.getByLabel('显示项',{exact:true});
  await display.selectOption('claude_gpt');
  await page.getByLabel('显示的账号').selectOption('__all__');
  await expect(display).toHaveValue('claude_gpt');
  await expect(page.getByRole('status')).toHaveCount(0);
  const address=page.getByLabel('Tools 服务地址');
  await address.fill(''); await expect(address).toHaveValue('');
  await address.fill('https://tools.example.com/prefix');
  await expect(page.getByText('服务地址已修改，请重新填写此服务的管理密钥。')).toBeVisible();
  await page.getByRole('button',{name:'返回额度一览'}).click();
  await page.getByRole('button',{name:'进入桌面模式'}).click();
  const selector=page.getByRole('combobox',{name:'Antigravity 查看账号'});
  await expect(selector).toBeVisible();
  await selector.selectOption('__all__');
  await expect(selector).toHaveValue('__all__');
  await expect(page.locator('.widget-toast')).toHaveCount(0);
  await expect(page.locator('.widget-platform.antigravity time')).toHaveCount(0);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.locator('.desktop-widget').screenshot({path:'.qa/widget-account-select-demo.png'});
  await selector.selectOption('demo-a');
  await expect(selector).toHaveValue('demo-a');
  await expect(page.locator('.widget-platform.antigravity time')).toHaveCount(1);
});

test('slow account switch retains the displayed values and global pool selection',async({page})=>{
  await page.setViewportSize({width:350,height:700});
  await page.goto('http://127.0.0.1:1420/tests/widget-harness.html');
  const selector=page.getByRole('combobox',{name:'Antigravity 查看账号'});
  await expect(page.getByText('80%',{exact:true})).toBeVisible();
  await selector.selectOption('__all__');
  await expect(page.getByText('更新中 · 旧值',{exact:true})).toBeVisible();
  await expect(page.getByText('80%',{exact:true})).toBeVisible();
  await expect(selector).toBeDisabled();
  await expect(page.getByText('Claude & GPT Models · 5 小时',{exact:true})).toBeVisible();
  await expect(page.locator('.widget-platform.antigravity time')).toHaveCount(1);
  expect(await page.locator('.desktop-widget').evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
  await page.locator('.desktop-widget').screenshot({path:'.qa/widget-pending-demo.png'});
  await page.evaluate(()=>{(window as unknown as {completeQuota:()=>void}).completeQuota();});
  await expect(page.getByText('40%',{exact:true})).toBeVisible();
  await expect(selector).toBeEnabled();
  await expect(page.locator('.widget-platform.antigravity time')).toHaveCount(0);
});


test('wallpaper materials use one surface with readable light and dark tints',async({page})=>{
  await page.setViewportSize({width:350,height:700});
  await page.addInitScript(()=>{
    const wallpaper='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="900" height="900"><defs><linearGradient id="g"><stop stop-color="#6a9aa6"/><stop offset="1" stop-color="#203b60"/></linearGradient></defs><rect width="900" height="900" fill="url(#g)"/><path d="M0 480L420 70 610 510 900 200V900H0" fill="#c1d4cb"/><path d="M0 660L500 310 900 700V900H0" fill="#627d92"/></svg>');
    Object.assign(window,{isTauri:true,__TAURI_INTERNALS__:{metadata:{currentWindow:{label:'main'}},transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(command:string)=>command==='desktop_background'?{revision:'demo-wallpaper',dataUrl:wallpaper,x:160,y:240,width:900,height:900,attached:true}:command==='list_antigravity_accounts'?[]:1}});
  });
  for(const tone of ['light','dark']){
    for(const glass of ['frosted','liquid']){
      await page.goto(`http://127.0.0.1:1420/tests/widget-harness.html?glass=${glass}&tone=${tone}`);
      const card=page.locator('.desktop-widget.has-wallpaper');await expect(card).toBeVisible();
      expect(await card.evaluate(el=>getComputedStyle(el).borderTopColor)).toBe('rgba(0, 0, 0, 0)');
      await expect(page.locator('.wallpaper-sample')).toHaveCount(1);
      expect(await page.locator('.wallpaper-sample').evaluate(el=>getComputedStyle(el).filter)).toContain(glass==='liquid'?'blur(8px)':'blur(18px)');
      await card.screenshot({path:`.qa/glass-${glass}-${tone}.png`});
    }
  }
});


test('live background uses native composition without static wallpaper requests',async({page})=>{
 await page.addInitScript(()=>{Object.assign(window,{isTauri:true,backdropCommands:[] as string[],__TAURI_INTERNALS__:{metadata:{currentWindow:{label:'main'}},transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd:string)=>{(window as unknown as {backdropCommands:string[]}).backdropCommands.push(cmd);return cmd==='desktop_live_status'?{error:null,frames:20,sources:1}:1;}}});});
 await page.goto('http://127.0.0.1:1420/tests/widget-harness.html?live=1');
 await expect(page.locator('.desktop-widget.live-background')).toBeVisible();
 await expect(page.locator('.wallpaper-sample')).toHaveCount(0);
 expect(await page.locator('.widget-backdrop').evaluate(el=>getComputedStyle(el).backgroundColor)).toBe('rgba(0, 0, 0, 0)');
 expect(await page.evaluate(()=>(window as unknown as {backdropCommands:string[]}).backdropCommands.includes('desktop_background'))).toBe(false);
});
