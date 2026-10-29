// Brand identity is independent of the service carrying the request.
export const brandNames:Record<string,string>={glm:'GLM / 智谱',deepseek:'DeepSeek',claude:'Claude',openai:'OpenAI / GPT',gemini:'Gemini',qwen:'Qwen / 通义',kimi:'Kimi',doubao:'豆包',minimax:'MiniMax',grok:'Grok',hunyuan:'混元',meta:'Meta / Llama',mistral:'Mistral',qoder:'Qoder',trae:'Trae',openrouter:'OpenRouter',siliconflow:'硅基流动'};
const aliases:Record<string,string>={zhipu:'glm',chatglm:'glm',anthropic:'claude',google:'gemini',alibaba:'qwen',moonshot:'kimi',bytedance:'doubao',xai:'grok',tencent:'hunyuan',llama:'meta',siliconcloud:'siliconflow'};
function identify(value:string):string|undefined{
 const lower=value.toLowerCase().trim();if(brandNames[lower])return lower;if(aliases[lower])return aliases[lower];
 const id=lower.split('/').pop()||lower;
 const rules:[RegExp,string][]=[[/^(?:glm|chatglm)(?:[-._\d]|$)/,'glm'],[/^deepseek(?:[-._\d]|$)/,'deepseek'],[/^claude(?:[-._\d]|$)/,'claude'],[/^(?:gpt|chatgpt|o[134])(?:[-._\d]|$)/,'openai'],[/^gemini(?:[-._\d]|$)/,'gemini'],[/^(?:qwen|qwq)(?:[-._\d]|$)/,'qwen'],[/^(?:kimi|moonshot)(?:[-._\d]|$)/,'kimi'],[/^doubao(?:[-._\d]|$)/,'doubao'],[/^minimax(?:[-._\d]|$)/,'minimax'],[/^grok(?:[-._\d]|$)/,'grok'],[/^hunyuan(?:[-._\d]|$)/,'hunyuan'],[/^(?:llama|meta-llama)(?:[-._\d]|$)/,'meta'],[/^(?:mistral|mixtral|codestral)(?:[-._\d]|$)/,'mistral']];
 return rules.find(([pattern])=>pattern.test(id))?.[1];
}
export function modelBrand(model:{id:string;target?:string;brand?:string;provider?:string}):string|undefined{const vendor=identify(model.provider||'');return identify(model.brand||'')||(vendor&&!['qoder','trae','openrouter','siliconflow'].includes(vendor)?vendor:undefined)||identify(model.target||'')||identify(model.id);}
export function brandIcon(id:string|undefined):string{return id?`/brands/${id}.svg`:'/brands/generic.svg';}
