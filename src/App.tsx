import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { ArrowLeft, ArrowUpRight, Check, ChevronDown, FlaskConical, KeyRound, Minus, Pin, RefreshCw, Settings2, ShieldCheck, X, Monitor } from 'lucide-react';
import { amount, toolsGroupMode, toolsViewMatches, displayMetrics, countdown, countdownDelay, dateText, demoSnapshots, initialState, platforms } from './types';
import Widget from './Widget';
import {AgentSettings,AgentRows} from './Agent2Api';
import type { ProviderId, Settings, Snapshot, ViewState } from './types';

const native = isTauri();
type Panel = 'general' | ProviderId | null;
interface ToolsAccount {id:string;label:string;available:boolean;groups:string[]}
export default function App() {
  const [state, setState] = useState<ViewState>(initialState);
  const [panel, setPanel] = useState<Panel>(null);
  const [demo, setDemo] = useState(false);
  const [demoData, setDemoData] = useState<Snapshot[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [busy, setBusy] = useState<string[]>([]);
  const [notice, setNotice] = useState('');
  const [now, setNow] = useState(Date.now() / 1000);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const stateRef=useRef(state);stateRef.current=state;
  const snapshotRef=useRef<Snapshot[]>([]);snapshotRef.current=demo?demoData:state.snapshots;
  useEffect(() => { document.querySelector('main')?.scrollTo(0, 0); }, [panel, demo]);
  function notify(message: string) { setNotice(message); clearTimeout(timer.current); timer.current = setTimeout(() => setNotice(''), 6500); }
  async function load() { if (native) setState(await invoke<ViewState>('get_state')); }
  function update(s: Snapshot) { setState(old => ({ ...old, snapshots: old.snapshots.map(p => p.provider === s.provider ? s : p) })); setBusy(old => old.filter(p => p !== s.provider)); }
  useEffect(() => {
    let disposed = false;
    const cleanup: (() => void)[] = [];
    if (native) {
      load().catch(() => notify('无法读取本地设置，请重启应用'));
      listen<Snapshot>('snapshot-updated', e => { if (!disposed) update(e.payload); }).then(fn => { if (disposed) fn(); else cleanup.push(fn); });
      listen<string>('refresh-started', e => { if (!disposed) setBusy(old => [...new Set([...old, e.payload])]); }).then(fn => { if (disposed) fn(); else cleanup.push(fn); });
      listen<Settings>('settings-updated',e=>{if(!disposed)setState(old=>({...old,settings:e.payload}));}).then(fn=>disposed?fn():cleanup.push(fn));
      listen<string>('desktop-error',e=>{if(!disposed)notify(e.payload);}).then(fn=>disposed?fn():cleanup.push(fn));
      listen<string>('widget-action',e=>{
        const s=stateRef.current.settings;
        const actions:Record<string,()=>unknown>={
          'widget-refresh':()=>refresh(),
          'widget-compact':()=>saveSettings({...s,compact:!s.compact}).catch(e=>notify(String(e))),
          'widget-glass':()=>saveSettings({...s,glass:s.glass==='frosted'?'liquid':'frosted'}).catch(e=>notify(String(e))),
          'widget-lock':()=>saveSettings({...s,positionLocked:!s.positionLocked}).catch(e=>notify(String(e))),
          'widget-reset':()=>invoke('reset_widget_position').catch(e=>notify(String(e)))
        };actions[e.payload]?.();
      }).then(fn=>disposed?fn():cleanup.push(fn));
    }
    let tick:ReturnType<typeof setTimeout>;
    const advance=()=>{setNow(Date.now()/1000);tick=setTimeout(advance,countdownDelay(snapshotRef.current));};
    advance();
    return () => { disposed = true; cleanup.forEach(fn => fn()); clearTimeout(tick); clearTimeout(timer.current); };
  }, []);
  async function refresh(id?: ProviderId) {
    if (demo) { setDemoData(demoSnapshots()); return; }
    if (!native) { notify('浏览器为界面预览，请打开桌面版连接账号'); return; }
    const ids = id ? [id] : platforms.filter(p => p.id === 'codex' || stateRef.current.configured[p.id]).map(p => p.id);
    await Promise.all(ids.map(async provider => {
      if (busy.includes(provider)) return;
      setBusy(old => [...new Set([...old, provider])]);
      try { update(await invoke<Snapshot>('refresh_provider', { provider })); }
      catch (e) { notify(String(e)); }
      finally { setBusy(old => old.filter(p => p !== provider)); }
    }));
  }
  async function openPlatform(id: ProviderId) {
    if (native) { try { await invoke('open_platform', { provider: id }); } catch (e) { notify(String(e)); } }
    else notify('请在桌面版中打开平台');
  }
  async function saveSettings(settings: Settings) {
    if (!native) { setState(old=>({...old,settings})); return; }
    await invoke('save_settings', { settings }); await load();
  }
  async function setMode(desktop:boolean){
    if(native){try{await invoke('set_display_mode',{desktop});await load();}catch(e){notify(String(e));}}
    else setState(old=>({...old,settings:{...old.settings,desktopMode:desktop}}));
    setPanel(null);
  }
  const snapshots = demo ? demoData : state.snapshots;
  const connected = snapshots.filter(s => s.status === 'ready').length;
  const latest = Math.max(0, ...snapshots.map(s => s.updatedAt || 0));
  function toggleDemo() { if (!demo) setDemoData(demoSnapshots()); setDemo(v => !v); setExpanded(null); }
  if(state.settings.desktopMode)return <><Widget settings={state.settings} snapshots={snapshots} busy={busy} now={now} demo={demo} onRefresh={refresh} onPanel={()=>setMode(false)} onSave={saveSettings} onError={notify}/>{notice&&<div role="status" className="widget-toast" onClick={()=>setNotice('')}>{notice}</div>}</>;
  return <div className="app-shell full-panel" style={{'--ui-scale':state.settings.fontScale/100} as React.CSSProperties}>
    <header className="titlebar">
      <div className="drag-zone" data-tauri-drag-region onMouseDown={e => { if (native && e.button === 0 && e.detail === 1) getCurrentWindow().startDragging().catch(() => {}); }}>
        <img className="app-icon" src="/app-icon.png" alt=""/><span>Token++</span><span className="version">{state.version.split('.').slice(0,2).join('.')}</span>
      </div>
      <div className="window-actions">
        <button className="icon-button" aria-label="进入桌面模式" title="桌面模式" onClick={()=>setMode(true)}><Monitor size={16}/></button>
        <button className={`icon-button ${state.settings.alwaysOnTop ? 'active' : ''}`} aria-label="窗口置顶" aria-pressed={state.settings.alwaysOnTop} title="窗口置顶" onClick={() => saveSettings({ ...state.settings, alwaysOnTop: !state.settings.alwaysOnTop }).catch(e => notify(String(e)))}><Pin size={15}/></button>
        <button className="icon-button" aria-label="最小化" title="最小化" onClick={() => native && getCurrentWindow().minimize().catch(() => notify('无法最小化窗口'))}><Minus size={16}/></button>
        <button className="icon-button close" aria-label="收起到托盘" title="收起到托盘" onClick={() => native && getCurrentWindow().hide().catch(() => notify('无法收起窗口'))}><X size={16}/></button>
      </div>
    </header>

    <main>
      {panel ? <SettingsPanel panel={panel} state={state} native={native} onBack={() => setPanel(null)} onChangePanel={setPanel} onSave={saveSettings} onReload={load} onOpen={openPlatform} onCredentialsSaved={async id => { await load(); await refresh(id); }} notify={notify}/> : <>
        <section className="intro">
          <div className="intro-top"><span className="section-label">我的 AI 账户</span><button className="icon-button settings-button" aria-label="打开设置" title="设置" onClick={() => setPanel('general')}><Settings2 size={19}/></button></div>
          <h1>额度，一眼看清。</h1>
          <div className="intro-bottom"><span><i className={`status-dot ${connected ? 'green' : ''}`}/>{demo ? '演示模式' : `${connected} 个平台已连接`}<span className="subtle"> / {platforms.length} 个平台</span></span><button className="text-button" onClick={() => refresh()} disabled={busy.length > 0}><RefreshCw size={13} className={busy.length ? 'spinning' : ''}/>{busy.length ? '刷新中' : '全部刷新'}</button></div>
        </section>
        {demo && <div className="demo-banner"><FlaskConical size={14}/><span>以下为演示数据，用于体验界面</span><button onClick={toggleDemo}>退出演示</button></div>}
        {!native && !demo && <div className="preview-banner">网页预览 · 打开桌面版后可连接真实账号</div>}
        <section className="provider-list" aria-label="平台额度">
          {platforms.map(p => {
            const s = snapshots.find(s => s.provider === p.id) || initialState().snapshots.find(s => s.provider === p.id)!;
            const isBusy = !demo && busy.includes(p.id);
            const isOpen = expanded === p.id;
            const stale = s.status === 'stale';
            const ready = s.metrics.length > 0 || !!s.gateway;
            const previousView=p.id==='antigravity'&&ready&&!toolsViewMatches(s,state.settings);
            const label = isBusy ? (previousView?'更新中 · 上次结果':'更新中') : previousView?'上次视图' : demo ? '演示' : ({ ready: '已连接', stale: '数据过期', error: '查询失败', unsupported: '待接入', idle: '待刷新', unconfigured: '未配置' }[s.status] || '待刷新');
            return <article key={p.id} className={`provider ${p.id} ${stale ? 'is-stale' : ''}`}>
              <div className="provider-heading"><div className="identity"><span className="provider-icon"><img src={p.icon} alt=""/></span><div><h2>{p.name}</h2><p>{p.subtitle}</p></div></div><span className={`badge ${s.status === 'ready' ? 'ok' : stale || s.status === 'error' ? 'warn' : ''}`}><i className={`status-dot ${s.status === 'ready' ? 'green' : ''}`}/>{label}</span></div>
              {p.id==='agent2api'&&s.gateway?<AgentRows snapshot={s} settings={state.settings} now={now}/>:ready ? <div className="metrics">{displayMetrics(s).map((m, i) => <div className="metric" key={`${m.label}-${i}`}>
                <div className="metric-line"><span className="metric-label">{m.label}</span><strong className={m.remainingPercent === null ? 'balance-value' : 'quota-value'}>{amount(m.value, m.unit)}{m.remainingPercent !== null && <small>剩余</small>}</strong></div>
                {m.remainingPercent !== null && <><div className="progress-track"><div className={m.remainingPercent < 15 ? 'low' : ''} style={{ width: `${m.remainingPercent}%` }}/></div><div className="reset-line">{p.id==='antigravity'&&(s.view?.account??state.settings.antigravityAccount)==='__all__'?'各账号独立重置':countdown(m.resetsAt, now)}</div></>}
                {p.id === 'huawei' && m.resetsAt && <p className="coupon-expiry">最近到期：{dateText(m.resetsAt)}（北京时间）</p>}
              </div>)}</div> : <div className="empty-metric"><span className="empty-value">—</span><span>{isBusy ? '正在读取额度…' : s.message}</span></div>}
              {stale || s.status === 'error' ? <div className="refresh-problem"><p className="error-message">{s.message}</p><button className="retry-button" aria-label={`重试 ${p.name}`} disabled={isBusy} onClick={()=>refresh(p.id)}><RefreshCw size={13} className={isBusy?'spinning':''}/>{isBusy?'重试中':'重试'}</button></div> : p.id === 'huawei' && ready ? <p className="provider-note">{s.message}</p> : null}
              <div className="provider-footer"><button className="details-button" aria-expanded={isOpen} onClick={() => setExpanded(isOpen ? null : p.id)}>{isOpen ? '收起详情' : '查看详情'}<ChevronDown size={13} className={isOpen ? 'rotated' : ''}/></button>{!ready ? <button className="connect-button" onClick={() => setPanel(p.id)}>{p.id === 'codex' ? '连接设置' : '连接账号'}<ArrowUpRight size={13}/></button> : <button className="icon-button small" aria-label={`刷新 ${p.name}`} title='刷新' disabled={isBusy} onClick={() => refresh(p.id)}><RefreshCw size={13} className={isBusy ? 'spinning' : ''}/></button>}</div>
              {isOpen && <div className="details"><dl>{s.details.map((d, i) => <div key={i}><dt>{d.label}</dt><dd>{d.value}</dd></div>)}{s.metrics.filter(m => m.remainingPercent !== null).map((m, i) => <div key={`time-${i}`}><dt>{m.label} 重置</dt><dd>{m.resetsAt ? `${dateText(m.resetsAt)}（北京时间）` : '时间未知'}</dd></div>)}<div><dt>上次成功更新</dt><dd>{s.updatedAt ? dateText(s.updatedAt) : '尚无数据'}</dd></div><div><dt>数据来源</dt><dd>{demo ? '演示数据' : p.id === 'codex' ? '本机官方 Codex' : p.id === 'antigravity' ? 'Antigravity Tools' : p.id==='agent2api'?'Agent2API 管理接口':'官方查询接口'}</dd></div></dl><div className="detail-actions"><button className="text-button" onClick={() => openPlatform(p.id)}>{p.id === 'antigravity' ? '打开官网' : '打开平台'}<ArrowUpRight size={12}/></button><button className="text-button" onClick={() => setPanel(p.id)}>连接设置</button></div></div>}
            </article>;
          })}
        </section>
        {!demo && !state.configured.deepseek && !state.configured.huawei && <div className="onboarding"><KeyRound size={16}/><div><strong>连接账号，开始查看</strong><p>在设置中添加密钥，凭据保存在本机。</p></div><button aria-label="配置账号" className="icon-button" onClick={() => setPanel('deepseek')}><ArrowUpRight size={17}/></button></div>}
        <footer className="dashboard-footer"><span>{latest ? `最近更新 ${dateText(latest)}` : '等待首次连接'}</span><button className={`text-button ${demo ? 'active' : ''}`} onClick={toggleDemo}><FlaskConical size={12}/>{demo ? '退出演示' : '体验演示'}</button></footer>
      </>}
    </main>
    <div className="bottom-bar"><ShieldCheck size={12}/><span>本机存储 · 凭据由 Windows 安全保管</span><span className="build-number">v{state.version}</span></div>
    {notice && <div role="status" className="toast">{notice}<button aria-label="关闭提示" onClick={() => setNotice('')}><X size={14}/></button></div>}
  </div>;
}

function SettingsPanel({ panel, state, native, onBack, onChangePanel, onSave, onReload, onOpen, onCredentialsSaved, notify }: { panel: Exclude<Panel, null>; state: ViewState; native: boolean; onBack: () => void; onChangePanel: (panel: Panel) => void; onSave: (settings: Settings) => Promise<void>; onReload:()=>Promise<void>; onOpen: (id: ProviderId) => Promise<void>; onCredentialsSaved: (id: ProviderId) => Promise<void>; notify: (message: string) => void }) {
  const [settings, setSettings] = useState(state.settings);
  const [key, setKey] = useState('');
  const [secret, setSecret] = useState('');
  const [saving, setSaving] = useState(false);
  const [autostart,setAutostart]=useState<boolean|null>(null);
  const [startupBusy,setStartupBusy]=useState(false);
  useEffect(()=>{
    if(panel!=='general')return;
    let disposed=false;
    if(!native){setAutostart(false);return;}
    setAutostart(null);
    invoke<boolean>('autostart_enabled').then(value=>{if(!disposed)setAutostart(value);}).catch(()=>{if(!disposed)notify('无法读取开机自启状态，请重新打开设置重试');});
    return()=>{disposed=true;};
  },[panel,native]);
  async function toggleAutostart(enabled:boolean){
    if(!native){notify('请在桌面版中设置开机自启');return;}
    setStartupBusy(true);
    try{setAutostart(await invoke<boolean>('set_autostart',{enabled}));notify(enabled?'已开启登录 Windows 后自动启动':'已关闭开机自启');}
    catch(error){setAutostart(await invoke<boolean>('autostart_enabled').catch(()=>null));notify(String(error));}
    finally{setStartupBusy(false);}
  }
  const [toolsAccounts,setToolsAccounts]=useState<ToolsAccount[]>([]);
  const [toolsError,setToolsError]=useState('');
  const [toolsAddress,setToolsAddress]=useState(state.settings.antigravityUrl||`http://127.0.0.1:${state.settings.antigravityPort}`);
  const toolsAddressChanged = settings.antigravityUrl !== state.settings.antigravityUrl;
  useEffect(() => { setSettings(state.settings);setToolsAddress(state.settings.antigravityUrl||`http://127.0.0.1:${state.settings.antigravityPort}`); }, [state.settings]);
  useEffect(() => { setKey(''); setSecret(''); }, [panel]);
  const p = platforms.find(p => p.id === panel);
  async function submit(e: React.FormEvent) {
    e.preventDefault(); if (!native) { if(panel==='general')await onSave(settings);else notify('请在桌面版中保存设置和凭据'); return; }
    setSaving(true);setToolsError('');
    try {
      if (panel === 'general' || panel === 'codex') { await onSave(settings); if (panel === 'codex') await onCredentialsSaved('codex'); }
      else if(panel==='antigravity'){
        await onSave(settings);
        if(key.trim()) {await invoke('save_credentials',{provider:panel,key,secret:''});setKey('');await onSave(settings);}
        const accounts=await invoke<ToolsAccount[]>('list_antigravity_accounts');setToolsAccounts(accounts);
        const available=accounts.filter(a=>a.available);
        const selected=settings.antigravityAccount==='__all__'?{id:'__all__'}:accounts.find(a=>a.id===settings.antigravityAccount) || (available.length===1?available[0]:undefined);
        if(!selected){notify('已读取账号，请选择要显示的账号后再次保存');return;}
        const next={...settings,antigravityAccount:selected.id,visiblePlatforms:settings.visiblePlatforms.includes('antigravity')?settings.visiblePlatforms:[...settings.visiblePlatforms,'antigravity' as ProviderId]};
        setSettings(next);await onSave(next);await onCredentialsSaved('antigravity');
      }
      else if (panel === 'deepseek' || panel === 'huawei') {
        if (panel === 'huawei') await onSave(settings);
        await invoke('save_credentials', { provider: panel, key, secret }); setKey(''); setSecret(''); notify('凭据已安全保存，正在查询'); await onCredentialsSaved(panel);
      }
    } catch (e) { if(panel==='antigravity')setToolsError(String(e));notify(String(e)); } finally { setSaving(false); }
  }
  async function remove() {
    setSaving(true);
    try { await invoke('remove_credentials', { provider: panel }); notify('凭据已移除'); await onCredentialsSaved(panel as ProviderId); }
    catch (e) { notify(String(e)); } finally { setSaving(false); }
  }
  return <section className="settings-page">
    <button className="text-button back-button" onClick={onBack}><ArrowLeft size={15}/>返回额度一览</button>
    <h1>连接与偏好</h1><p className="settings-subtitle">把需要的额度，放在手边。</p>
    <nav className="settings-tabs" aria-label="设置分类">{[{ id: 'general', name: '通用' }, ...platforms].map(p => <button key={p.id} className={panel === p.id ? 'selected' : ''} onClick={() => onChangePanel(p.id as Panel)}>{p.name}</button>)}</nav>
    <form onSubmit={submit}>
      <h2>{p ? p.name : '桌面与刷新'}</h2>
      {panel==='agent2api'&&<AgentSettings state={state} native={native} onSave={onSave} onConnected={onReload} onError={notify}/>}
      {panel === 'general' && <>
        <label className="switch-row"><span><strong>开机自启</strong><small>{autostart===null?'正在读取 Windows 设置…':'登录 Windows 后启动，修改立即生效'}</small></span><input type="checkbox" checked={autostart===true} disabled={startupBusy||autostart===null} onChange={e=>toggleAutostart(e.target.checked)}/></label>
        <div className="info-box">自动启动时恢复桌面组件；如果上次使用完整面板，则先收起到托盘。建议安装后再开启，避免移动程序导致启动路径失效。</div>
        <label className="switch-row"><span><strong>桌面模式</strong><small>显示在壁纸层，隐藏任务栏按钮</small></span><input type="checkbox" checked={settings.desktopMode} onChange={e=>setSettings({...settings,desktopMode:e.target.checked})}/></label>
        <label className="switch-row"><span><strong>桌面精简显示</strong><small>每项 1–3 行；关闭后使用标准布局</small></span><input type="checkbox" checked={settings.compact} onChange={e=>setSettings({...settings,compact:e.target.checked})}/></label>
        <label className="switch-row"><span><strong>动态壁纸原生毛玻璃</strong><small>由 Windows 实时合成背景；需要 Windows 11 并开启系统透明效果</small></span><input type="checkbox" checked={settings.liveBackground} onChange={e=>setSettings({...settings,liveBackground:e.target.checked})}/></label>
        <label className="field">桌面背景材质<select value={settings.glass} onChange={e=>setSettings({...settings,glass:e.target.value as Settings['glass']})}><option value="frosted">毛玻璃</option><option value="liquid">液态玻璃</option></select></label>
        <div className="settings-grid"><label className="field">玻璃明暗<select value={settings.glassTone} onChange={e=>setSettings({...settings,glassTone:e.target.value as Settings['glassTone']})}><option value="light">浅色</option><option value="dark">深色</option></select></label><label className="field">底色不透明度<select value={settings.opacity} onChange={e=>setSettings({...settings,opacity:Number(e.target.value)})}>{[35,50,65,80,95].map(n=><option value={n} key={n}>{n}%</option>)}</select></label></div>
        <label className="field">全局字体大小<select value={settings.fontScale} onChange={e=>setSettings({...settings,fontScale:Number(e.target.value)})}>{[90,100,110,120,130,140].map(n=><option key={n} value={n}>{n}%{n===100?'（默认已放大）':''}</option>)}</select></label>
        <label className="switch-row"><span><strong>锁定桌面位置</strong><small>右键菜单或托盘可随时恢复操作</small></span><input type="checkbox" checked={settings.positionLocked} onChange={e=>setSettings({...settings,positionLocked:e.target.checked})}/></label>
        <label className="switch-row"><span><strong>显示最近代金券到期日</strong></span><input type="checkbox" checked={settings.showExpiry} onChange={e=>setSettings({...settings,showExpiry:e.target.checked})}/></label>
        <fieldset className="platform-options"><legend>桌面显示的平台</legend>{platforms.map(p=><label key={p.id}><input type="checkbox" checked={settings.visiblePlatforms.includes(p.id)} onChange={e=>setSettings({...settings,visiblePlatforms:e.target.checked?[...settings.visiblePlatforms,p.id]:settings.visiblePlatforms.filter(id=>id!==p.id)})}/>{p.name}</label>)}</fieldset>
        <div className="info-box">静态模式使用壁纸文件。实时模式由 Windows 原生合成动态背景，需要 Windows 11 并开启系统透明效果。</div>
        <label className="switch-row"><span><strong>窗口置顶</strong><small>工作时保持额度小卡片可见</small></span><input type="checkbox" checked={settings.alwaysOnTop} onChange={e => setSettings({ ...settings, alwaysOnTop: e.target.checked })}/></label>
        <label className="field">余额与代金券刷新间隔<span className="input-unit"><input required type="number" min="1" max="120" value={settings.balanceMinutes} onChange={e => setSettings({ ...settings, balanceMinutes: Number(e.target.value) })}/><span>分钟</span></span></label>
        <label className="field">订阅额度刷新间隔<span className="input-unit"><input required type="number" min="1" max="120" value={settings.quotaMinutes} onChange={e => setSettings({ ...settings, quotaMinutes: Number(e.target.value) })}/><span>分钟</span></span></label>
        <div className="info-box">关闭按钮会将窗口收起到托盘。点击托盘图标可重新打开，右键菜单可退出。</div>
      </>}
      {panel === 'deepseek' && <>
        <div className="connection-state"><i className={`status-dot ${state.configured.deepseek ? 'green' : ''}`}/>{state.configured.deepseek ? '已保存 API Key，输入新值可替换' : '尚未配置 API Key'}</div>
        <label className="field">API Key<input name="deepseekKey" required type="password" value={key} onChange={e => setKey(e.target.value)} autoComplete="off" spellCheck={false} placeholder="输入 DeepSeek 官方 API Key"/></label>
        <div className="info-box">查询总余额、充值余额与赠金余额。密钥仅发送给 DeepSeek 官方接口。</div>
      </>}
      {panel === 'huawei' && <>
        <div className="connection-state"><i className={`status-dot ${state.configured.huawei ? 'green' : ''}`}/>{state.configured.huawei ? '已保存 AK / SK，输入新值可替换' : '尚未配置 AK / SK'}</div>
        <label className="field">账号站点<select value={settings.huaweiSite} onChange={e => setSettings({ ...settings, huaweiSite: e.target.value })}><option value="cn">华为云中国站</option><option value="intl">华为云国际站</option></select></label>
        <label className="field">Access Key（AK）<input required type="password" value={key} onChange={e => setKey(e.target.value)} autoComplete="off" spellCheck={false} placeholder="输入访问密钥 ID"/></label>
        <label className="field">Secret Key（SK）<input required type="password" value={secret} onChange={e => setSecret(e.target.value)} autoComplete="off" spellCheck={false} placeholder="输入秘密访问密钥"/></label>
        <div className="info-box">使用具备优惠券查询权限的 AK / SK（billing:coupon:view）。模型推理 API Key 不能代替账务凭据。<br/><br/>按全部可用代金券的剩余额度统计，未筛选 MaaS 适用范围。</div>
      </>}
      {panel === 'codex' && <>
        <div className="connection-state"><Check size={14}/>复用本机官方账号登录</div>
        <label className="field">Codex 程序路径（可选）<input value={settings.codexPath} onChange={e => setSettings({ ...settings, codexPath: e.target.value })} spellCheck={false} placeholder="留空自动查找 codex.exe"/></label>
        <div className="info-box">通过官方 Codex 客户端读取订阅额度。请先在此电脑登录 ChatGPT 账号。窗口时长与重置时间按账号实际数据展示。</div>
      </>}
      {panel==='antigravity'&&<>
        <div className="connection-state"><i className={`status-dot ${state.configured.antigravity&&!toolsAddressChanged?'green':''}`}/>{state.configured.antigravity&&!toolsAddressChanged?'已保存此服务的管理密钥，可留空继续使用':'填写 Tools 服务地址与管理密钥'}</div>
        <div className="info-box">支持本机和云端 Tools，无需启动 Antigravity 官方程序。请使用 Tools 管理密码；未设置独立管理密码时使用 API Key。服务需提供账号与额度管理接口。</div>
        <label className="field">Tools 服务地址<input type="text" required value={toolsAddress} onChange={e=>{setToolsAddress(e.target.value);setToolsAccounts([]);setToolsError('');setSettings({...settings,antigravityUrl:e.target.value,antigravityAccount:''});}} spellCheck={false} autoCapitalize="none" autoComplete="off" aria-describedby="tools-address-help"/></label>
        <p id="tools-address-help" className="privacy-note">本机可填 localhost:8045；云端使用 https://域名，可带部署路径。不要填写 /v1 模型中转地址。</p>
        {toolsAddressChanged&&<div className="info-box">服务地址已修改，请重新填写此服务的管理密钥。</div>}
        <label className="field">Tools 管理密钥<input type="password" required={!state.configured.antigravity||toolsAddressChanged} value={key} onChange={e=>setKey(e.target.value)} autoComplete="off" spellCheck={false} placeholder="在此填写，保存后不会回显"/></label>
        {toolsError&&<div role="alert" className="info-box">{toolsError}</div>}
        <label className="field">Tools 额度刷新间隔（分钟）<input type="number" required min="1" max="120" value={settings.antigravityMinutes} onChange={e=>setSettings({...settings,antigravityMinutes:Number(e.target.value)})}/></label>
        <label className="field">显示的账号<select value={settings.antigravityAccount} onChange={e=>setSettings({...settings,antigravityAccount:e.target.value})}><option value="">保存连接后读取账号，单账号自动选择</option><option value="__all__">综合平均（所有可查询账号等权）</option>{settings.antigravityAccount&&settings.antigravityAccount!=='__all__'&&!toolsAccounts.some(a=>a.id===settings.antigravityAccount)&&<option value={settings.antigravityAccount}>已保存的账号（保存连接可重新获取列表）</option>}{toolsAccounts.map((a,i)=><option key={a.id} value={a.id} disabled={!a.available}>{i+1}. {a.label}{a.available?'':' · 已停用或受限'}</option>)}</select></label>
        <label className="field">显示项<select aria-label="显示项" value={toolsGroupMode(settings.antigravityGroups)} onChange={e=>setSettings({...settings,antigravityGroups:e.target.value==='gemini'?['Gemini Models']:e.target.value==='claude_gpt'?['Claude & GPT Models']:[]})}><option value="all">两者都显示</option><option value="gemini">Gemini Models</option><option value="claude_gpt">Claude &amp; GPT Models</option>{toolsGroupMode(settings.antigravityGroups)==='custom'&&<option value="custom" disabled>保留现有显示项</option>}</select></label>
        <p className="privacy-note">此选项统一应用于所有账号与综合平均，切换账号后保持不变。</p>
        <div className="info-box">可在桌面标题旁切换单个账号或综合平均。综合按账号等权，短期与每周、不同额度池分别统计；各账号独立重置。服务不可达时保留旧数据。</div>
      </>}
      {panel!=='agent2api'&&<button className="primary-button" disabled={saving} type="submit">{saving ? <RefreshCw size={15} className="spinning"/> : <ShieldCheck size={15}/>} {saving ? '处理中…' : panel === 'general' ? '保存设置' : panel === 'codex' ? '保存并读取额度' : panel==='antigravity'?'保存连接并读取额度':'安全保存并连接'}</button>}
      {p && <button className="platform-link text-button" type="button" onClick={() => onOpen(p.id)}>{p.id === 'antigravity' ? '打开 Antigravity 官网' : `打开 ${p.name} 平台`}<ArrowUpRight size={13}/></button>}
      {(panel === 'deepseek' || panel === 'huawei' || panel==='antigravity') && state.configured[panel] && <button className="remove-button" type="button" disabled={saving} onClick={remove}>移除本机保存的凭据</button>}
    </form>
    <p className="privacy-note"><ShieldCheck size={14}/>保存后不会回显密钥。凭据不写入配置文件。</p>
  </section>;
}
