import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { amount, toolsGroupMode, toolsViewMatches, displayMetrics, countdown, countdownDelay, dateText, initialState, demoSnapshots } from './types.ts';
describe('display accuracy', () => {
  it('does not invent an allowance before connection', () => { assert.ok(initialState().snapshots.every(s => s.metrics.length === 0 && s.updatedAt === null)); });
  it('orders Antigravity short windows before weekly without changing data or groups',()=>{
    const snapshot=demoSnapshots()[3];
    const metric=snapshot.metrics[0];
    snapshot.metrics=[{...metric,label:'Gemini · 每周',value:'42',resetsAt:200},{...metric,label:'Gemini · 5 小时',value:'81',resetsAt:100},{...metric,label:'Claude · 每周'},{...metric,label:'Claude · 5h limit'}];
    const ordered=displayMetrics(snapshot);
    assert.deepEqual(ordered.map(m=>m.label),['Gemini · 5 小时','Gemini · 每周','Claude · 5h limit','Claude · 每周']);
    assert.equal(ordered[0],snapshot.metrics[1]);assert.equal(ordered[1].resetsAt,200);assert.equal(snapshot.metrics[0].value,'42');
  });
  it('keeps reached reset times pending confirmation', () => { assert.match(countdown(1000, 1001), /待确认/); assert.equal(countdown(null), '重置时间未知'); });
  it('uses seconds and handles long windows', () => { assert.equal(countdown(1000 + 7200, 1000), '2 小时 0 分钟后重置'); assert.equal(countdown(1000 + 172800, 1000), '2 天 0 小时后重置'); });
  it('formats Beijing time regardless of machine timezone', () => { assert.match(dateText(0), /08:00/); });
  it('keeps currencies distinct', () => { assert.equal(amount('1.2','CNY'), '¥ 1.20'); assert.equal(amount('1.2','USD'), '$ 1.20'); assert.equal(amount('0',''),'0.00'); });
  it('updates countdowns infrequently until a reset is close',()=>{
    const snapshots=demoSnapshots();const now=Date.now()/1000;
    assert.equal(countdownDelay(snapshots,now),30000);
    snapshots[2].metrics[0].resetsAt=Math.floor(now)+30;
    assert.equal(countdownDelay(snapshots,now),1000);
  });
});

describe('Tools global display preferences and view identity',()=>{
  it('recognizes existing pool names and both global choices',()=>{
    assert.equal(toolsGroupMode([]),'all');
    assert.equal(toolsGroupMode(['Gemini Models']),'gemini');
    assert.equal(toolsGroupMode(['Claude & GPT Models']),'claude_gpt');
    assert.equal(toolsGroupMode(['Gemini','Claude']),'all');
    assert.equal(toolsGroupMode(['custom pool']),'custom');
  });
  it('identifies old data without relabelling its account, service or pool',()=>{
    const settings=initialState().settings;settings.antigravityAccount='account-a';settings.antigravityGroups=['Gemini Models'];
    const snapshot=demoSnapshots()[3];snapshot.view={account:'account-a',groups:['Gemini Models'],service:'http://127.0.0.1:8045'};
    assert.equal(toolsViewMatches(snapshot,settings),true);
    assert.equal(toolsViewMatches(snapshot,{...settings,antigravityAccount:'__all__'}),false);
    assert.equal(toolsViewMatches(snapshot,{...settings,antigravityGroups:['Claude & GPT Models']}),false);
    assert.equal(toolsViewMatches(snapshot,{...settings,antigravityUrl:'https://tools.example.com'}),false);
    assert.equal(snapshot.view.account,'account-a');
  });
});
