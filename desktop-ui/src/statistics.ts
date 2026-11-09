/** Display facts without turning absent historical measurements into zero. */
export type Summary=Record<string,unknown>;
export function metricValue(values:Summary,key:string):number|null {
 const value=values[key];if(typeof value!=='number'||!Number.isFinite(value)||value<0)return null;
 const unknown=Array.isArray(values.legacy_unknown_fields)?values.legacy_unknown_fields:[];
 if((key==='actions_performed'||key==='pages_processed')&&!values.effort_started_at)return null;
 if(key==='days'&&value===0&&(unknown.includes('started_at')||['checks','active_seconds','offices_seen','booking_screens'].some(k=>typeof values[k]==='number'&&Number(values[k])>0)))return null;
 if(value===0&&unknown.includes(key))return null;
 return value;
}
export function formatDuration(seconds:number|null,language:string):string {
 if(seconds===null||!Number.isFinite(seconds)||seconds<0)return '—';
 const minutes=Math.floor(seconds/60);const hours=Math.floor(minutes/60);const unit=language==='ru'?['ч','мин']:['h','min'];
 return hours?`${hours} ${unit[0]}${minutes%60?` ${minutes%60} ${unit[1]}`:''}`:`${minutes} ${unit[1]}`;
}
export function stamp(raw:unknown,language:string):string {
 if(typeof raw!=='string')return '—';const value=new Date(raw);return Number.isNaN(value.valueOf())?'—':new Intl.DateTimeFormat(language==='en'?'en-GB':language,{year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit',hourCycle:'h23',timeZone:'Europe/Madrid'}).format(value);
}
export function firstEvent(values:Summary,prefix:string,period:string,language:string,t:(key:string)=>string){
 const when=values[prefix+'_at'];const seconds=values[prefix+'_seconds'];const quality=values[prefix+'_active_quality'];const status=values[prefix+'_status'];const base=prefix==='first_office'?'Первый офис':'Первый выбор даты';
 const caption=t(base+(period==='today'?' сегодня':status==='recorded'?' в истории':''));
 let value=typeof when==='string'?stamp(when,language):t(status==='unknown'?'Дата неизвестна':'Ещё не появлялись');
 const hints:string[]=[];
 if(typeof when==='string'&&typeof seconds==='number'&&Number.isFinite(seconds)&&seconds>=0){const duration=seconds<60?`${Math.floor(seconds)} ${t('с')}`:formatDuration(seconds,language);value+=' · '+(quality==='minimum'?'≥ ':'')+t('{duration} поиска').replace('{duration}',duration);}
 if(status==='recorded')hints.push(t('Первое событие в сохранённой истории; более ранние события могли не сохраниться.'));
 if(quality==='minimum')hints.push(t('Не менее указанного времени: учтены подтверждённые интервалы, в истории есть пропуски.'));
 else if(typeof when==='string'&&seconds==null)hints.push(t('Дата известна; длительность активного поиска до события не сохранена.'));
 return {caption,value,hint:hints.join('\n')};
}
export function metricHint(values:Summary,key:string,language:string,t:(key:string)=>string):string {
 if(key==='actions_performed'||key==='pages_processed')return values.effort_started_at?t('Учёт с {time}. Ранние действия и страницы не восстанавливаются.').replace('{time}',stamp(values.effort_started_at,language)):t('Ранее этот показатель не сохранялся.');
 return Array.isArray(values.legacy_unknown_fields)&&values.legacy_unknown_fields.includes(key)?t('До обновления этот показатель не сохранялся; показаны новые наблюдения.'):'';
}

export function observedDuration(seconds:number,language:string,t:(key:string)=>string):string {return seconds<60?`${Math.floor(Math.max(0,seconds))} ${t('с')}`:formatDuration(seconds,language);}

/** Rows of the overview groups: key, Russian label, icon. Counters follow the period filter. */
export const statisticGroups:{title:string;icon:string;rows:[string,string,string][]}[]=[
 {title:'Доступность',icon:'eye',rows:[['offices_seen','Появлений офисов','office'],['unique_offices','Разных офисов','pin'],['booking_screens','Появления выбора даты','calendar_days'],['banners','«Сит нет» до запроса','clipboard'],['no_slots','Ответы «Сит нет» на запрос записи','message'],['successes','Получено сит','calendar']]},
 {title:'Ошибки и восстановление',icon:'wrench',rows:[['server_errors','Ошибок сервера','server'],['rate_limited','Ограничений HTTP 429','clock'],['blocked','Ограничения доступа','shield'],['flow_passes','Повторные проходы','restart'],['cod_oper_count','Ответы «Сит нет» с cod. oper','shield-alert']]},
];
