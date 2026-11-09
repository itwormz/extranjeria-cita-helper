/** Presentation rules for today's appointment budget (Python 0.3.6 `window.py` / `statistics_panel.py`). Pure: no DOM, no IPC. */
export interface BudgetOffice {at:string;names:string[];result:string|null}
export interface BudgetEpisode {first:string;last:string;recovered:string|null;offices:BudgetOffice[]}
export interface BudgetSummary {
 wait_reason:'window'|'minimum_pause'|'cod_oper'|null;attempt_window:[string,string]|null;target_source:'manual'|'adaptive'|'base';
 manual_target:number|null;adaptive_enabled:boolean;adaptive_target:number|null;base_target:number;target:number;sent:number;remaining:number;
 disabled:boolean;banners:number;no_slots:number;immediate_attempt:boolean;cod_oper_count:number;ready:string;restricted:boolean;offices:number;
 next_target:number|null;episode_history:BudgetEpisode[];episode_offices:BudgetOffice[];first_cod_oper:string|null;last_cod_oper:string|null;
}
export interface SearchState {active:boolean;manual:boolean;paused?:boolean}
type Translate=(key:string)=>string;
const ZONE='Europe/Madrid';
export const MAX_DAILY_TARGET=2147483647;

/** Fill `{name}` placeholders; unknown names stay as written. */
export function fmt(template:string,values:Record<string,string|number>):string{return template.replace(/\{(\w+)\}/g,(whole,name)=>name in values?String(values[name]):whole);}
/** A plain integer 1…2147483647 without sign, spaces, separators or fraction; anything else is rejected. */
export function parseDailyTarget(text:string):number|null{if(!/^[1-9][0-9]{0,9}$/.test(text))return null;const value=Number(text);return value<=MAX_DAILY_TARGET?value:null;}

function parts(value:string){
 const f=new Intl.DateTimeFormat('en-GB',{timeZone:ZONE,year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hourCycle:'h23'}).formatToParts(new Date(value));
 const get=(type:string)=>f.find(p=>p.type===type)?.value??'00';
 return {date:`${get('year')}-${get('month')}-${get('day')}`,time:`${get('hour')}:${get('minute')}`};
}
const clock=(value:string)=>parts(value).time;
const stamp=(value:string)=>{const p=parts(value);return `${p.date} ${p.time}`;};

export function budgetLine(b:BudgetSummary,t:Translate):string{
 let text=fmt(t('Бюджет {target} · Остаток {remaining}'),{target:b.target,remaining:b.remaining});
 if(b.disabled)text+=' · '+t('Без распределения');
 if(b.restricted)text+='\n'+fmt(t('Возможное ограничение · Проба с {time}'),{time:clock(b.ready)});
 return text;
}
export function sourceKey(b:BudgetSummary):string{return {manual:'Задан вручную',adaptive:'Адаптивный бюджет',base:'Базовый бюджет'}[b.target_source]??'Базовый бюджет';}
export function waitReasonKey(reason:BudgetSummary['wait_reason']|undefined):string|null{
 return reason?({window:'Ждём окно распределения',minimum_pause:'Пауза между попытками',cod_oper:'Пауза после cod. oper'}[reason]??null):null;
}
export function attemptWindowLabel(b:BudgetSummary|null|undefined):{caption:string;text:string;visible:boolean}{
 if(!b)return {caption:'Окно',text:'',visible:false};
 const waiting=Boolean(b.wait_reason);const window=b.restricted?null:b.attempt_window;
 let text='';
 if(window)text=`${clock(window[0])}–${clock(window[1])}`;
 if(waiting)text=clock(b.ready);
 else if(b.immediate_attempt)text='Открыто сейчас';
 return {caption:waiting?'До':'Окно',text,visible:Boolean(window||waiting||b.immediate_attempt)};
}
/** Try now needs a running search that the program, not the person, controls. */
export function canTryNow(search:SearchState|null|undefined):boolean{return Boolean(search?.active&&!search.manual);}
export function showTryNowButton(search:SearchState|null|undefined,b:BudgetSummary|null|undefined):boolean{return canTryNow(search)&&(b?.wait_reason==='window'||b?.wait_reason==='cod_oper');}
/** Continue searching only while the person owns the browser of a running search (booking handoff). */
export function canResume(search:SearchState|null|undefined):boolean{return Boolean(search?.active&&search.manual);}

const OUTCOME:Record<string,string>={cod_oper:'Сит нет · cod oper',booking:'Показан выбор даты',request_no_slots:'Сит нет',request_no_slots_restart:'Сит нет',post_contact_no_slots:'Сит нет'};
/** Lines of the restriction history, newest first, one blank line between episodes. */
export function historyLines(b:BudgetSummary,t:Translate):string[]{
 const episodes=[...b.episode_history];
 if(b.first_cod_oper)episodes.push({first:b.first_cod_oper,last:b.last_cod_oper??b.first_cod_oper,offices:b.episode_offices,recovered:null});
 const lines:string[]=[];
 for(const episode of episodes.reverse()){
  lines.push(fmt(t('Первый cod oper: {first} · Последний: {last}'),{first:stamp(episode.first),last:stamp(episode.last)}));
  if(episode.recovered)lines.push(fmt(t('Восстановление между {last} и {recovered}'),{last:stamp(episode.last),recovered:stamp(episode.recovered)}));
  for(const office of episode.offices)lines.push(`${clock(office.at)} · ${office.names.join(', ')} · ${t(OUTCOME[office.result??'']??'Исход неизвестен')}`);
  lines.push('');
 }
 return lines;
}
