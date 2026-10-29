import {AgentRows} from './Agent2Api';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LockKeyhole, PanelTop, RefreshCw, MoreHorizontal, AlertCircle } from 'lucide-react';
import type { Settings, Snapshot, ProviderId } from './types';
import { amount, toolsViewMatches, displayMetrics, countdown, dateText, platforms } from './types';

interface Background {revision: string;dataUrl: string | null;x:number;y:number;width:number;height:number;attached:boolean}
interface ToolsAccount {id:string;label:string;available:boolean;groups:string[]}
type Placement=Pick<Background,'x'|'y'|'width'|'height'>;
export default function Widget({settings,snapshots,busy,now,demo,onRefresh,onPanel,onSave,onError}:{settings:Settings;snapshots:Snapshot[];busy:string[];now:number;demo:boolean;onRefresh:(id?:ProviderId)=>void;onPanel:()=>void;onSave:(s:Settings)=>Promise<void>;onError:(s:string)=>void}){
  const root=useRef<HTMLDivElement>(null);
  const wallpaperElement=useRef<HTMLDivElement>(null);
  const revision=useRef('');
  const [background,setBackground]=useState<Background|null>(null);
  const [image,setImage]=useState('');
  const [imageReady,setImageReady]=useState(false);
  const [previewMenu,setPreviewMenu]=useState(false);
  const [liveError,setLiveError]=useState('');
  const native=isTauri();
  const [toolsAccounts,setToolsAccounts]=useState<ToolsAccount[]>([]);
  const [accountBusy,setAccountBusy]=useState(false);
  const toolsVisible=settings.visiblePlatforms.includes('antigravity')&&snapshots.some(s=>s.provider==='antigravity'&&s.status!=='unconfigured');
  useEffect(()=>{
    let disposed=false;
    setToolsAccounts([]);
    if(demo){setToolsAccounts([{id:'demo-a',label:'演示账号 A',available:true,groups:[]},{id:'demo-b',label:'演示账号 B',available:true,groups:[]}]);return;}
    if(native&&toolsVisible)invoke<ToolsAccount[]>('list_antigravity_accounts').then(accounts=>{if(!disposed)setToolsAccounts(accounts);}).catch(()=>{/* Existing account and combined choice remain usable while service is offline. */});
    return()=>{disposed=true;};
  },[settings.antigravityUrl,settings.antigravityPort,toolsVisible,demo,native]);
  useEffect(()=>{
    setLiveError('');if(!native||!settings.liveBackground)return;
    let disposed=false;const poll=()=>invoke<{error:string|null}>('desktop_live_status').then(s=>{if(!disposed)setLiveError(s.error?'实时背景暂不可用，可关闭后重新开启':'');}).catch(()=>{if(!disposed)setLiveError('实时背景状态不可用');});
    poll();const timer=setInterval(poll,2000);return()=>{disposed=true;clearInterval(timer);};
  },[native,settings.liveBackground]);
  async function changeAccount(account:string){
    setAccountBusy(true);
    try{await onSave({...settings,antigravityAccount:account});onRefresh('antigravity');}
    catch(error){onError(String(error));}finally{setAccountBusy(false);}
  }
  useEffect(()=>{
    document.documentElement.classList.add('widget-active');
    let disposed=false;
    let pending:ReturnType<typeof setTimeout>|undefined;
    let frame:number|undefined;
    let latestPlacement:Placement|null=null;
    let placementBusy=false;
    let loading=false;
    const unsubs:(()=>void)[]=[];
    function place(placement:Placement){
      latestPlacement=placement;
      if(frame!==undefined)return;
      frame=requestAnimationFrame(()=>{
        frame=undefined;
        const node=wallpaperElement.current,p=latestPlacement;
        if(disposed||!node||!p)return;
        node.style.setProperty('--wallpaper-x',`${-p.x}px`);node.style.setProperty('--wallpaper-y',`${-p.y}px`);
        node.style.setProperty('--wallpaper-width',`${p.width}px`);node.style.setProperty('--wallpaper-height',`${p.height}px`);
      });
    }
    async function reposition(){
      pending=undefined;
      if(disposed||placementBusy)return;
      placementBusy=true;
      try{place(await invoke<Placement>('desktop_placement'));}catch{/* Next native move or periodic sync retries. */}
      finally{placementBusy=false;}
    }
    async function sync(){
      if(!native||settings.liveBackground||disposed||loading)return;
      loading=true;
      try{ const b=await invoke<Background>('desktop_background',{knownRevision:revision.current});if(disposed)return;revision.current=b.revision;if(b.dataUrl!==null)setImage(b.dataUrl);setBackground(b); }
      catch{ /* A readable tint is retained if the wallpaper is not a supported file. */ }
      finally{loading=false;}
    }
    revision.current="";setImage("");setImageReady(false);sync();const timer=setInterval(sync,30000);
    if(native){
      getCurrentWindow().onMoved(()=>{if(pending===undefined)pending=setTimeout(reposition,32);}).then(fn=>disposed?fn():unsubs.push(fn));
      listen<Placement>('desktop-placement',e=>{if(!disposed)place(e.payload);}).then(fn=>disposed?fn():unsubs.push(fn));
      listen('desktop-moved',()=>{sync();invoke('remember_widget_position').catch(()=>onError('位置保存失败，可从托盘重置'));}).then(fn=>disposed?fn():unsubs.push(fn));
    }
    return()=>{disposed=true;clearInterval(timer);clearTimeout(pending);if(frame!==undefined)cancelAnimationFrame(frame);unsubs.forEach(fn=>fn());document.documentElement.classList.remove('widget-active');};
  },[settings.liveBackground]);
  useEffect(()=>{
    setImageReady(false);if(!image)return;
    let disposed=false;const probe=new Image();probe.onload=()=>{if(!disposed)setImageReady(true);};probe.src=image;
    return()=>{disposed=true;};
  },[image]);
  useLayoutEffect(()=>{
    const node=root.current;if(!node)return;
    let handle:ReturnType<typeof setTimeout>;
    let last='';
    const resize=new ResizeObserver(()=>{clearTimeout(handle);handle=setTimeout(()=>{const height=Math.ceil(node.getBoundingClientRect().height);if(!native)return;const width=Math.round(350*settings.fontScale/100);const radius=parseFloat(getComputedStyle(node).borderTopLeftRadius);const key=`${width}:${height}:${radius}:${devicePixelRatio}`;if(last===key)return;last=key;invoke('resize_widget',{height,width,radius}).catch(()=>onError('组件尺寸调整失败'));},80);});
    resize.observe(node);
    let unlisten:(()=>void)|undefined;let disposed=false;
    if(native)getCurrentWindow().onScaleChanged(()=>{last="";resize.unobserve(node);resize.observe(node);}).then(fn=>{if(disposed)fn();else unlisten=fn;});
    return()=>{disposed=true;unlisten?.();clearTimeout(handle);resize.disconnect();};
  },[settings.fontScale,settings.glass,settings.liveBackground,native]);
  const selected=platforms.filter(p=>settings.visiblePlatforms.includes(p.id));
  const shown=selected.filter(p=>{const s=snapshots.find(s=>s.provider===p.id);return s&&s.status!=='unconfigured';});
  const vars={'--glass-opacity':settings.opacity/100,'--ui-scale':settings.fontScale/100} as CSSProperties;
  async function context(){if(native){try{await invoke('desktop_menu');}catch(e){onError(String(e));}}else setPreviewMenu(v=>!v);}
  return <div ref={root} className={`desktop-widget ${settings.glass} ${settings.glassTone} ${settings.liveBackground?"live-background":""} ${imageReady?'has-wallpaper':''} ${settings.compact?'compact':'standard'}`} style={vars} onContextMenu={e=>{e.preventDefault();context();}}>
    <div ref={wallpaperElement} className="widget-backdrop" aria-hidden="true" style={background?{'--wallpaper-x':`${-background.x}px`,'--wallpaper-y':`${-background.y}px`,'--wallpaper-width':`${background.width}px`,'--wallpaper-height':`${background.height}px`} as CSSProperties:undefined}>{imageReady&&<><div className="wallpaper-sample" style={{backgroundImage:`url("${image}")`}}/></>}<div className="glass-tint"/></div>
    <div className="widget-content">
      <header className="widget-header"><div className={`widget-grip ${settings.positionLocked?'locked':''}`} title={settings.positionLocked?'位置已锁定':'拖动移动位置'} onMouseDown={e=>{if(e.button===0&&!settings.positionLocked&&native)invoke('desktop_drag').catch(e=>onError(String(e)));}}><img className="widget-app-icon" src="/app-icon.png" alt=""/><span className="widget-brand">Token++</span>{settings.positionLocked&&<LockKeyhole size={11}/>}</div><div className="widget-actions"><button aria-label="刷新桌面额度" title="刷新" onClick={()=>onRefresh()} disabled={busy.length>0}><RefreshCw size={13} className={busy.length?'spinning':''}/></button><button aria-label="打开完整面板" title="完整面板" onClick={onPanel}><PanelTop size={14}/></button><button aria-label="桌面菜单" title="右键菜单" onClick={context}><MoreHorizontal size={16}/></button></div></header>
      {liveError&&<div className="widget-warning" role="status">{liveError}</div>}
      {demo&&<div className="widget-demo">演示数据</div>}
      <section className="widget-platforms" aria-label="桌面额度">
        {shown.map(p=>{const s=snapshots.find(s=>s.provider===p.id)!;const problem=['stale','error'].includes(s.status);const pending=p.id==='antigravity'&&!toolsViewMatches(s,settings)&&s.metrics.length>0;const aggregate=p.id==='antigravity'&&(s.view?.account??settings.antigravityAccount)==='__all__';return <div className={`widget-platform ${p.id} ${aggregate?'aggregate':''} ${problem?'problem':''}`} key={p.id} aria-busy={busy.includes(p.id)}>
          <div className="widget-row"><span className="widget-name"><i className={`widget-dot ${problem?'warn':s.status==='ready'?'good':''}`}/><img className="widget-platform-icon" src={p.icon} alt=""/>{p.name}{p.id==='antigravity'&&<select className="widget-account-select" aria-label="Antigravity 查看账号" title={settings.antigravityAccount==='__all__'?'综合平均：各账号等权':toolsAccounts.find(a=>a.id===settings.antigravityAccount)?.label||'选择账号'} value={settings.antigravityAccount} disabled={accountBusy||busy.includes('antigravity')} onChange={e=>changeAccount(e.target.value)}><option value="" disabled>选择账号</option><option value="__all__">综合平均</option>{settings.antigravityAccount&&settings.antigravityAccount!=='__all__'&&!toolsAccounts.some(a=>a.id===settings.antigravityAccount)&&<option value={settings.antigravityAccount}>{s.details.find(d=>d.label==='账号')?.value||'当前账号'}</option>}{toolsAccounts.map(a=><option key={a.id} value={a.id} disabled={!a.available}>{a.label}{a.available?'':'（受限）'}</option>)}</select>}</span>{p.id==='agent2api'?null:s.metrics[0]?.remainingPercent===null?<strong className="widget-amount">{s.metrics.map(m=>amount(m.value,m.unit)).join(' / ')}</strong>:!s.metrics.length?<span className="widget-empty">{s.status==='unsupported'?'待接入':'等待查询'}</span>:<span className="widget-kind" role={busy.includes(p.id)?'status':undefined}>{busy.includes(p.id)?(pending?'更新中 · 旧值':'更新中'):pending?'上次结果':'剩余额度'}</span>}</div>
          {p.id==='agent2api'&&s.gateway&&<AgentRows snapshot={s} settings={settings} now={now} compact dense={settings.compact}/>}
          {displayMetrics(s).filter(m=>m.remainingPercent!==null).map((m,i)=><div className="widget-quota" key={`${m.label}-${i}`}><div className="widget-quota-line"><span title={m.label}>{settings.compact?(m.label.split('·').pop()?.trim()||m.label):m.label.replace(/^codex\s*[·・]?\s*/i,'')}</span><strong>{amount(m.value,m.unit)}</strong>{!aggregate&&<time title={m.resetsAt?`${dateText(m.resetsAt)} 重置（北京时间）`:'重置时间未知'}>{countdown(m.resetsAt,now).replace('后重置','后')}</time>}</div><div className="widget-track" role="progressbar" aria-label={`${p.name} ${m.label}剩余`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={m.remainingPercent!}><span className={m.remainingPercent!<15?'low':''} style={{width:`${m.remainingPercent}%`}}/></div></div>)}
          {p.id==='huawei'&&settings.showExpiry&&s.metrics[0]?.resetsAt&&<div className={`widget-expiry ${s.metrics[0].resetsAt-now<7*86400?'near':''}`}>最近到期 {dateText(s.metrics[0].resetsAt)}</div>}
          {problem&&!busy.includes(p.id)&&<div className="widget-problem"><button className="widget-warning" onClick={onPanel} title={s.message}><AlertCircle size={12}/>{pending?'上次视图 · 查看更新状态':s.status==='stale'?'旧数据 · 更新未成功':'查询异常 · 查看原因'}</button><button className="retry-button" aria-label={`重试 ${p.name}`} disabled={busy.includes(p.id)} onClick={()=>onRefresh(p.id)}><RefreshCw size={12} className={busy.includes(p.id)?'spinning':''}/>{busy.includes(p.id)?'重试中':'重试'}</button></div>}
        </div>;})}
        {!shown.length&&<button className="widget-setup" onClick={onPanel}>打开完整面板，连接要显示的账号</button>}
      </section>
    </div>
    {previewMenu&&<div className="widget-preview-menu"><button onClick={onPanel}>打开完整面板</button><button onClick={()=>onSave({...settings,compact:!settings.compact})}>切换精简 / 标准</button><button onClick={()=>onSave({...settings,glass:settings.glass==='frosted'?'liquid':'frosted'})}>切换玻璃材质</button><button onClick={()=>onSave({...settings,glassTone:settings.glassTone==='light'?'dark':'light'})}>切换明暗</button><button onClick={()=>setPreviewMenu(false)}>关闭菜单</button></div>}
  </div>;
}
