export type ProviderId = 'deepseek' | 'huawei' | 'codex' | 'antigravity';
export interface Metric { label: string; value: string; unit: string; remainingPercent: number | null; resetsAt: number | null }
export interface QuotaView { account: string; groups: string[]; service: string }
export interface Snapshot { view?: QuotaView | null; provider: ProviderId; status: string; message: string; updatedAt: number | null; metrics: Metric[]; details: { label: string; value: string }[] }
export interface Settings { alwaysOnTop: boolean; balanceMinutes: number; quotaMinutes: number; huaweiSite: string; codexPath: string; antigravityPort: number; antigravityUrl: string; antigravityAccount: string; antigravityGroups: string[]; antigravityMinutes: number; desktopMode: boolean; compact: boolean; liveBackground: boolean; glass: 'frosted' | 'liquid'; glassTone: 'light' | 'dark'; opacity: number; fontScale: number; positionLocked: boolean; showExpiry: boolean; visiblePlatforms: ProviderId[]; desktopPosition?: [number,number] | null; panelBounds?: [number,number,number,number] | null }
export interface ViewState { settings: Settings; snapshots: Snapshot[]; configured: Partial<Record<ProviderId, boolean>>; version: string }
export const platforms: { id: ProviderId; name: string; subtitle: string; icon: string }[] = [
  { id: 'deepseek', name: 'DeepSeek', subtitle: 'API 余额', icon: '/brands/deepseek.svg' },
  { id: 'huawei', name: '华为云', subtitle: 'ModelArts MaaS · 代金券', icon: '/brands/huawei.png' },
  { id: 'codex', name: 'Codex', subtitle: 'Coding Plan', icon: '/brands/codex.svg' },
  { id: 'antigravity', name: 'Antigravity', subtitle: 'Google · Coding Plan', icon: '/brands/antigravity.svg' },
];
export function countdown(at: number | null, now = Date.now() / 1000): string {
  if (!at) return '重置时间未知';
  const seconds = Math.ceil(at - now);
  if (seconds <= 0) return '已到重置时间，待确认';
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟后重置`;
  const hours = Math.floor(minutes / 60);
  return hours >= 24 ? `${Math.floor(hours / 24)} 天 ${hours % 24} 小时后重置` : `${hours} 小时 ${minutes % 60} 分钟后重置`;
}
export function dateText(at: number): string {
  return new Intl.DateTimeFormat('zh-CN', { timeZone: 'Asia/Shanghai', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false }).format(at * 1000);
}
export function amount(value: string, unit: string): string {
  if (unit === '%') return `${value}%`;
  const n = Number(value);
  return `${unit === 'CNY' ? '¥' : unit === 'USD' ? '$' : unit} ${Number.isFinite(n) ? n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '—'}`;
}
export function initialState(): ViewState {
  return { version: '0.7.1', configured: {}, settings: { alwaysOnTop: false, balanceMinutes: 10, quotaMinutes: 3, huaweiSite: 'cn', codexPath: '', antigravityPort: 8045, antigravityUrl: '', antigravityAccount: '', antigravityGroups: [], antigravityMinutes: 5, desktopMode: false, compact: true, liveBackground: false, glass: 'frosted', glassTone: 'light', opacity: 65, fontScale: 100, positionLocked: false, showExpiry: true, visiblePlatforms: ['deepseek','huawei','codex'] }, snapshots: platforms.map(p => ({ provider: p.id, status: p.id === 'codex' ? 'idle' : 'unconfigured', message: p.id === 'antigravity' ? '连接 Antigravity Tools 后读取额度' : p.id === 'codex' ? '读取本机官方登录额度' : '配置后查看真实余额', updatedAt: null, metrics: [], details: [] })) };
}

export function countdownDelay(snapshots: Snapshot[], now = Date.now()/1000): number {
  return snapshots.some(s => s.metrics.some(m => m.remainingPercent !== null && m.resetsAt !== null && m.resetsAt > now && m.resetsAt - now < 60)) ? 1000 : 30000;
}

export function displayMetrics(snapshot: Snapshot): Metric[] {
  if(snapshot.provider!=='antigravity')return snapshot.metrics;
  const groups=new Map<string,number>();
  const items=snapshot.metrics.map((metric,index)=>{
    const parts=metric.label.split('·');const window=parts.pop()?.trim()||'';
    const group=parts.join('·').trim()||metric.label;
    if(!groups.has(group))groups.set(group,groups.size);
    const order=/^(5\s*小时|5h(?:\s*limit)?)$/i.test(window)?0:/^(每周|weekly|7\s*天)$/i.test(window)?1:2;
    return {metric,index,group:groups.get(group)!,order};
  });
  return items.sort((a,b)=>a.group-b.group||a.order-b.order||a.index-b.index).map(item=>item.metric);
}
export function demoSnapshots(): Snapshot[] {
  const now = Math.floor(Date.now() / 1000);
  const metric = (label: string, value: string, unit: string, remainingPercent: number | null = null, resetsAt: number | null = null): Metric => ({ label, value, unit, remainingPercent, resetsAt });
  return [
    { provider: 'deepseek', status: 'ready', message: '官方 API', updatedAt: now, metrics: [metric('可用余额', '86.42', 'CNY')], details: [{ label: '充值余额', value: '80.00 CNY' }, { label: '赠金余额', value: '6.42 CNY' }] },
    { provider: 'huawei', status: 'ready', message: '2 张可用 · 按全部可用代金券统计', updatedAt: now, metrics: [metric('代金券余额', '128.50', 'CNY', null, now + 7 * 86400)], details: [{ label: 'MaaS 体验代金券', value: '100.00 CNY · 7 天后到期' }, { label: '活动代金券', value: '28.50 CNY · 30 天后到期' }, { label: '统计口径', value: '全部可用代金券，未筛选 MaaS 适用范围' }] },
    { provider: 'codex', status: 'ready', message: '本机登录', updatedAt: now, metrics: [metric('Codex · 5 小时', '72', '%', 72, now + 7800), metric('Codex · 每周', '43', '%', 43, now + 2 * 86400 + 10800)], details: [] },
    { provider: 'antigravity', status: 'ready', message: '演示额度', updatedAt: now, metrics: [metric('Gemini 共享额度', '85', '%', 85, now + 13200)], details: [{ label: '数据来源', value: '示例数据，用于体验布局' }] },
  ];
}

export function toolsGroupMode(groups:string[]):'all'|'gemini'|'claude_gpt'|'custom' {
  if(!groups.length)return 'all';
  const kinds=new Set(groups.map(g=>{
    const key=g.toLowerCase().replace(/[^a-z0-9]/g,'');
    return ['gemini','geminimodels'].includes(key)?'gemini':['claude','gpt','claudemodels','claudegpt','claudegptmodels'].includes(key)?'claude_gpt':'custom';
  }));
  return kinds.has('custom')?'custom':kinds.size===2?'all':kinds.has('gemini')?'gemini':'claude_gpt';
}
export function toolsViewMatches(snapshot:Snapshot,settings:Settings):boolean {
  if(!snapshot.view)return true;
  const service=settings.antigravityUrl||`http://127.0.0.1:${settings.antigravityPort}`;
  return snapshot.view.account===settings.antigravityAccount&&snapshot.view.service===service&&JSON.stringify([...snapshot.view.groups].sort())===JSON.stringify([...settings.antigravityGroups].sort());
}
