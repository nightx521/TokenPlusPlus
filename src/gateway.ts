import type {GatewayAccount, GatewayData, Settings} from './types';
export function selectedAccounts(data:GatewayData|null|undefined,settings:Settings):GatewayAccount[]{
 if(!data || data.service!==settings.agent2apiUrl.replace(/\/$/,''))return [];
 const selection=settings.agent2apiSelection;
 return data.accounts.filter(a=>selection.accounts===null?a.enabled:selection.accounts.includes(a.id)).map(a=>{
  const chosen=selection.models[a.id];const models=a.models.filter(m=>chosen===undefined?m.enabled:chosen.includes(m.id));
  const metrics=a.metrics.filter(m=>selection.metrics[a.id]===undefined||selection.metrics[a.id].includes(m.id)).filter(m=>m.shared||m.modelIds.some(id=>models.some(model=>model.id===id||model.target===id)));
  return {...a,models,metrics};
 });
}
