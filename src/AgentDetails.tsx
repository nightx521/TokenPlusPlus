import {useEffect,useState} from 'react';import {invoke,isTauri} from '@tauri-apps/api/core';import {listen} from '@tauri-apps/api/event';
import {ModelBadges} from './Agent2Api';import {amount,countdown,dateText,initialState} from './types';import type {GatewayAccount,ViewState} from './types';
export function DetailsList({account}:{account:GatewayAccount}){const [search,setSearch]=useState('');const [now,setNow]=useState(Date.now()/1000);useEffect(()=>{const t=setInterval(()=>setNow(Date.now()/1000),30000);return()=>clearInterval(t);},[]);return <>
 <h1>{account.name}</h1><p className="agent-muted">{account.provider} · 全部额度项目</p><div className="agent-models"><ModelBadges models={account.models} compact/></div>
 <label className="field">搜索详情<input type="search" value={search} onChange={e=>setSearch(e.target.value)} placeholder="按资源包或额度名称筛选"/></label>
 <p className="privacy-note">总览与资源包是不同统计层级，请勿重复相加。</p>
 <div className="agent-details-list">{account.metrics.filter(m=>m.label.toLowerCase().includes(search.toLowerCase())).map(m=><section className="agent-reading" key={m.id}><div className="agent-reading-line"><span>{m.label}</span><strong>{amount(m.value,m.unit)}</strong></div>{m.remainingPercent!==null&&<div className="progress-track" role="progressbar" aria-label={m.label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={m.remainingPercent}><span style={{width:`${m.remainingPercent}%`}}/></div>}{m.resetsAt&&<p className="agent-muted">{countdown(m.resetsAt,now)} · {dateText(m.resetsAt)}</p>}</section>)}</div>
 {!account.metrics.length&&<p>{account.message||'暂无数据'}</p>}{account.updatedAt&&<p className="agent-muted">最近更新 {dateText(account.updatedAt)}</p>}
 </>;}
export default function AgentDetails(){const [state,setState]=useState<ViewState>(initialState);const [id,setId]=useState(new URLSearchParams(location.search).get('account')||'');const [error,setError]=useState('');
 useEffect(()=>{if(!isTauri()){setError('请在桌面版中打开账号详情');return;}let disposed=false;const clean:(()=>void)[]=[];const load=()=>invoke<ViewState>('get_state').then(s=>{if(!disposed)setState(s);}).catch(()=>{if(!disposed)setError('读取详情失败，请关闭后重试');});load();for(const event of ['snapshot-updated','settings-updated'])listen(event,load).then(fn=>disposed?fn():clean.push(fn));listen<string>('agent-details-account',e=>{setId(e.payload);load();}).then(fn=>disposed?fn():clean.push(fn));return()=>{disposed=true;clean.forEach(fn=>fn());};},[]);
 const account=state.snapshots.find(s=>s.provider==='agent2api')?.gateway?.accounts.find(a=>a.id===id);
 return <main className="agent-details-window" style={{fontSize:15*state.settings.fontScale/100}}>{error?<p role="alert">{error}</p>:account?<DetailsList account={account}/>:<p role="status">正在读取账号详情…</p>}</main>;
}
