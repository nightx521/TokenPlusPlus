import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {RefreshCw} from 'lucide-react';
import {amount,countdown,dateText} from './types';
import type {GatewayData,GatewayModel,Settings,Snapshot,ViewState} from './types';
import {brandIcon,brandNames,modelBrand} from './brands';
import {selectedAccounts} from './gateway';

export function ModelBadges({models,compact}:{models:GatewayModel[];compact:boolean}) {
 if(!compact)return <>{models.map(m=><span className="agent-model" key={m.id} title={`${m.name}${m.target!==m.id?` → ${m.target}`:''}`}><img src={brandIcon(modelBrand(m))} alt=""/>{m.name}</span>)}</>;
 const groups=new Map<string,string[]>();for(const model of models){const brand=modelBrand(model)||'unknown';groups.set(brand,[...(groups.get(brand)||[]),model.name]);}
 return <>{[...groups].map(([brand,names])=><span className="agent-model" key={brand} title={names.join('、')}><img src={brandIcon(brand==='unknown'?undefined:brand)} alt=""/>{brandNames[brand]?.split(' / ')[0]||'未知厂商'}</span>)}</>;
}

export function AgentRows({snapshot,settings,now,compact=false,dense=false}:{snapshot:Snapshot;settings:Settings;now:number;compact?:boolean;dense?:boolean}){
 const accounts=selectedAccounts(snapshot.gateway,settings);
 const [detailsError,setDetailsError]=useState('');
 async function details(id:string){if(nativeDetails()){try{await invoke('open_agent2api_details',{account:id});}catch(e){setDetailsError(String(e));}}else{window.open(`/index.html?view=agent-details&account=${encodeURIComponent(id)}`,'_blank','noopener');}}
 return <div className={`agent-accounts ${compact?'agent-compact':''} ${dense?'agent-dense':''}`}>
  {detailsError&&<p role="alert" className="agent-warning">{detailsError}</p>}
  {!accounts.length&&<p className="agent-muted">尚未选择显示的账号，请在连接设置中勾选。</p>}
  {accounts.map(a=>{const primary=a.metrics.find(m=>m.id==='available')||a.metrics[0];return <section className="agent-account" key={a.id} aria-label={`Agent2API 账号 ${a.name}`}>
   <div className="agent-account-title"><strong title={a.provider}>{a.name}</strong>{dense&&primary?<strong>{amount(primary.value,primary.unit)}</strong>:<span>{a.provider||'渠道未知'}</span>}<button type="button" className="agent-details-button" aria-label={`查看 ${a.name} 全部详情`} onClick={()=>details(a.id)}>详情 ›</button></div>
   <div className="agent-models"><ModelBadges models={a.models} compact={compact}/>{!a.models.length&&<span className="agent-muted">模型关联未知或未选择</span>}</div>
   {(dense?[]:primary?[primary]:[]).map(m=><div className="agent-reading" key={m.id}><div className="agent-reading-line"><span>{m.label}{m.shared&&!m.label.includes('共享')&&<small> · 共享</small>}</span><strong>{amount(m.value,m.unit)}</strong></div>
    {m.remainingPercent!==null&&<div className={compact?'widget-track':'progress-track'} role="progressbar" aria-label={`${a.name} ${m.label}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={m.remainingPercent}><span style={{width:`${m.remainingPercent}%`}}/></div>}
    {m.resetsAt&&<div className="agent-muted">{countdown(m.resetsAt,now)}</div>}
   </div>)}
   {dense&&primary?.resetsAt&&<time className="agent-muted">{countdown(primary.resetsAt,now)}</time>}
   {!a.metrics.length&&<p className="agent-muted">{a.message||'未选择额度项'}</p>}
   {a.status!=='ready'&&a.metrics.length>0&&<p role="status" className="agent-warning">{a.message||'上次结果，等待确认'}</p>}
   {!compact&&a.updatedAt&&<div className="agent-muted">更新于 {dateText(a.updatedAt)} · {a.status==='ready'?'已确认':a.status==='unsupported'?'暂无可用数值':'旧数据'}</div>}
  </section>})}
 </div>;
}
function nativeDetails(){return '__TAURI_INTERNALS__' in window;}

export function AgentSettings({state,native,onSave,onConnected,onError}:{state:ViewState;native:boolean;onSave:(s:Settings)=>Promise<void>;onConnected:()=>Promise<void>;onError:(s:string)=>void}){
 const [address,setAddress]=useState(state.settings.agent2apiUrl);const [token,setToken]=useState('');const [working,setWorking]=useState(false);const [search,setSearch]=useState('');const [data,setData]=useState<GatewayData|undefined>(state.snapshots.find(s=>s.provider==='agent2api')?.gateway??undefined);const [error,setError]=useState('');
 useEffect(()=>{const next=state.snapshots.find(s=>s.provider==='agent2api')?.gateway;if(next?.service===state.settings.agent2apiUrl)setData(next);},[state.snapshots,state.settings.agent2apiUrl]);
 const settings=state.settings;const selection=settings.agent2apiSelection;const changed=address.trim()!==settings.agent2apiUrl;
 async function connect(){if(!native){onError('请在桌面版中连接管理接口');return;}setWorking(true);setError('');try{
   const next={...settings,agent2apiUrl:address.trim(),agent2apiSelection:changed?{version:1,accounts:null,models:{},metrics:{}}:selection,visiblePlatforms:settings.visiblePlatforms.includes('agent2api')?settings.visiblePlatforms:[...settings.visiblePlatforms,'agent2api' as const]};
   await onSave(next);if(token.trim()){await invoke('save_credentials',{provider:'agent2api',key:token,secret:''});setToken('');}
   const snapshot=await invoke<Snapshot>('refresh_provider',{provider:'agent2api'});setData(snapshot.gateway??undefined);if(snapshot.status!=='ready')setError(snapshot.message);await onConnected();
  }catch(e){setError(String(e));}finally{setWorking(false);}}
 async function change(next:Settings['agent2apiSelection']){setError('');try{await onSave({...settings,agent2apiSelection:next});}catch(e){setError(String(e));}}
 const visible=data?.service===settings.agent2apiUrl&&!changed?data.accounts:[];
 const selected=(id:string,enabled:boolean)=>selection.accounts===null?enabled:selection.accounts.includes(id);
 function subset(id:string,field:'models'|'metrics',values:string[],checked:boolean,item:string){const current=selection[field][id]??values;return {...selection,[field]:{...selection[field],[id]:checked?[...new Set([...current,item])]:current.filter(v=>v!==item)}};}
 return <div className="agent-settings">
  <p className="info-box">读取账号额度与模型目录，不调用模型。需要管理访问令牌；模型调用 Key 可能没有此权限。令牌失效后请手动更新。</p>
  <label className="field">Agent2API 服务地址<input value={address} onChange={e=>{setAddress(e.target.value);setError('');}} autoComplete="off" spellCheck={false} placeholder="https://服务域名 或 localhost:3065"/></label>
  <label className="field">管理访问令牌<input type="password" value={token} onChange={e=>setToken(e.target.value)} autoComplete="off" spellCheck={false} placeholder={state.configured.agent2api&&!changed?'已保存，可留空继续使用':'仅在应用内填写，保存后不回显'}/></label>
  <button type="button" className="primary-button" disabled={working||!address.trim()||((changed||!state.configured.agent2api)&&!token.trim())} onClick={connect}>{working&&<RefreshCw size={14} className="spinning"/>}{working?'正在连接…':'测试连接并读取额度'}</button>
  {error&&<p className="error-message" role="alert">{error}</p>}
  <label className="field">额度刷新间隔（分钟）<input type="number" min={1} max={120} value={settings.agent2apiMinutes} onChange={e=>onSave({...settings,agent2apiMinutes:Number(e.target.value)}).catch(e=>setError(String(e)))}/></label>
  <p className="privacy-note">显示选择同时作用于桌面和完整面板；共享额度按账号显示一次。</p>
  {!!visible?.length&&<>
   <label className="field">搜索账号或模型<input type="search" value={search} onChange={e=>setSearch(e.target.value)}/></label>
   <div className="agent-select-actions"><button type="button" onClick={()=>change({...selection,accounts:visible.map(a=>a.id)})}>全部账号</button><button type="button" onClick={()=>change({...selection,accounts:[]})}>清空账号</button></div>
   {visible.filter(a=>`${a.name} ${a.provider} ${a.models.map(m=>m.name).join(' ')}`.toLowerCase().includes(search.toLowerCase())).map(a=><fieldset className="agent-choice" key={a.id}><legend><label><input type="checkbox" checked={selected(a.id,a.enabled)} onChange={e=>change({...selection,accounts:e.target.checked?[...new Set([...(selection.accounts??visible.filter(a=>a.enabled).map(a=>a.id)),a.id])]:(selection.accounts??visible.filter(a=>a.enabled).map(a=>a.id)).filter(id=>id!==a.id)})}/>{a.name} · {a.provider||'未知渠道'}</label></legend>
    <div className="agent-select-actions"><span>模型</span><button type="button" onClick={()=>change({...selection,models:{...selection.models,[a.id]:a.models.map(m=>m.id)}})}>全选模型</button><button type="button" onClick={()=>change({...selection,models:{...selection.models,[a.id]:[]}})}>清空模型</button></div>
    {a.models.map(m=><label className="agent-check" key={m.id}><input type="checkbox" checked={selection.models[a.id]?.includes(m.id)??m.enabled} onChange={e=>change(subset(a.id,'models',a.models.filter(m=>m.enabled).map(m=>m.id),e.target.checked,m.id))}/><img src={brandIcon(modelBrand(m))} alt=""/><span>{m.name}</span></label>)}
    {!a.models.length&&<p className="agent-muted">上游未提供明确的模型关联</p>}
    <div className="agent-select-actions"><span>额度项</span><button type="button" onClick={()=>change({...selection,metrics:{...selection.metrics,[a.id]:a.metrics.map(m=>m.id)}})}>全选额度项</button><button type="button" onClick={()=>change({...selection,metrics:{...selection.metrics,[a.id]:[]}})}>清空额度项</button></div>
    {a.metrics.map(m=><label className="agent-check" key={m.id}><input type="checkbox" checked={selection.metrics[a.id]?.includes(m.id)??true} onChange={e=>change(subset(a.id,'metrics',a.metrics.map(m=>m.id),e.target.checked,m.id))}/>{m.label} · {m.unit}</label>)}
   </fieldset>)}
  </>}
  <details className="agent-brand-library"><summary>可识别的模型厂商图标</summary><div className="agent-models">{Object.entries(brandNames).map(([id,label])=><span key={id} className="agent-model"><img src={brandIcon(id)} alt=""/>{label}</span>)}</div><p className="privacy-note">图标用于识别模型，不代表已接入对应官方余额查询。</p></details>
  {state.configured.agent2api&&<button className="remove-button" type="button" disabled={working} onClick={async()=>{try{await invoke('remove_credentials',{provider:'agent2api'});setData(undefined);await onConnected();}catch(e){onError(String(e));}}}>移除此服务的令牌</button>}
 </div>;
}
