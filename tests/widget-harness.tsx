import React, { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import Widget from '../src/Widget';
import { initialState } from '../src/types';
import type { Settings, Snapshot } from '../src/types';
import '../src/style.css';
import '../src/desktop.css';

// A controllable data source keeps the production widget mounted during a slow response.
function Harness() {
  const params=new URLSearchParams(location.search);
  const initial = {...initialState().settings,liveBackground:params.get("live")==="1",glass:params.get("glass")==="liquid"?"liquid" as const:"frosted" as const,glassTone:params.get("tone")==="dark"?"dark" as const:"light" as const, desktopMode:true, visiblePlatforms:['antigravity'] as Settings['visiblePlatforms'], antigravityAccount:'demo-a', antigravityGroups:['Claude & GPT Models']};
  const [settings,setSettings]=useState(initial);
  const current=useRef(settings);
  const [busy,setBusy]=useState<string[]>([]);
  const [snapshot,setSnapshot]=useState<Snapshot>({provider:'antigravity',status:'ready',message:'演示数据',updatedAt:1800000000,metrics:[{label:'Claude & GPT Models · 5 小时',value:'80',unit:'%',remainingPercent:80,resetsAt:1800006000}],details:[],view:{account:'demo-a',groups:initial.antigravityGroups,service:'http://127.0.0.1:8045'}});
  async function save(next:Settings){current.current=next;setSettings(next);}
  function refresh(){
    setBusy(['antigravity']);
    Object.assign(window,{completeQuota:()=>{
      setSnapshot(old=>({...old,metrics:old.metrics.map(m=>({...m,value:'40',remainingPercent:40})),view:{...old.view!,account:current.current.antigravityAccount}}));
      setBusy([]);
    }});
  }
  return <Widget settings={settings} snapshots={[snapshot]} busy={busy} now={1800000000} demo onRefresh={refresh} onSave={save} onError={error=>{throw new Error(error);}} onPanel={()=>{}}/>;
}
createRoot(document.getElementById('widget-test-root')!).render(<Harness/>);
