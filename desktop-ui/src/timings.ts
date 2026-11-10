export type Timings=Record<string,number|number[]|boolean>;
export type TimingInputs=Record<string,string[]>;
export const sequences=new Set(['expired_seconds','server_button_seconds','server_retry_seconds','blocked_seconds','timeout_retry_seconds']);
export interface InputFormat{decimals?:number;comma?:boolean}
export function timingInputs(values:Timings,format:InputFormat={}):TimingInputs{
 const show=(n:number)=>{if(format.decimals===undefined)return String(n);const text=n.toFixed(format.decimals);return format.comma?text.replace('.',','):text;};
 return Object.fromEntries(Object.entries(values).map(([key,value])=>[key,typeof value==='boolean'?[String(value)]:Array.isArray(value)?sequences.has(key)?[value.join(', ')]:value.map(show):[show(value)]]));
}
export function parseTimingInputs(fields:TimingInputs,shape:Timings):Timings{
 return Object.fromEntries(Object.entries(shape).map(([key,value])=>{
  if(typeof value==='boolean'){const flag=fields[key]?.[0];if(flag!=='true'&&flag!=='false')throw new Error(key);return [key,flag==='true'];}
  const parts=sequences.has(key)?fields[key]?.[0]?.split(','):fields[key];
  if(!parts?.length)throw new Error(key);
  const numbers=parts.map(part=>{const normalized=part.trim().replace(',','.');if(!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(normalized))throw new Error(key);const number=Number(normalized);if(!Number.isFinite(number))throw new Error(key);return number;});
  if(!sequences.has(key)&&numbers.length!==(Array.isArray(value)?2:1))throw new Error(key);
  return [key,Array.isArray(value)?numbers:numbers[0]];
 }));
}
export const timingGroups=[
 {title:'Ввод данных',fields:[['action_delay_seconds','Между действиями'],['typing_ms','Ввод NIE и имени'],['contact_typing_ms','Ввод контактов']]},
 {title:'Интервалы проверки',fields:[['bootstrap_seconds','Начальная загрузка / F5'],['reload_seconds','Интервал проверки'],['navigation_seconds','Таймаут страницы']]},
 {title:'Восстановление',fields:[['no_slots_restart_seconds','Новый проход · Aceptar'],['clave_error_seconds','Ошибка Cl@ve'],['rate_limit_seconds','HTTP 429'],['recovery_jitter_seconds','Добавочная пауза'],['expired_seconds','Истёкшая сессия'],['server_button_seconds','Кнопка ошибки'],['server_retry_seconds','Ошибка без кнопки'],['blocked_seconds','Блокировка'],['timeout_retry_seconds','Клик не отправлен'],['post_timeout_seconds','Ожидание после клика'],['restart_seconds','Перезапуск после таймаута']]},
 {title:'Уведомления',fields:[['alert_seconds','Повтор звонка']]}
];
/** Full field titles of Python 0.3.6 (`TIMING_FIELDS`), shown as the tooltip of each timing card. */
export const timingTitles:Record<string,string>={
 action_delay_seconds:'Между действиями, с',bootstrap_seconds:'Начальная загрузка / F5, с',typing_ms:'Между символами NIE и имени, мс',contact_typing_ms:'Между символами контактов, мс',
 reload_seconds:'Проверка сит: интервал, с',navigation_seconds:'Таймаут загрузки страницы, с',no_slots_restart_seconds:'Нет сит с Aceptar: пауза нового прохода, с',
 clave_error_seconds:'Ошибка Cl@ve: пауза перед новым окном, с',rate_limit_seconds:'HTTP 429: пауза перед пробой, с',alert_seconds:'Повтор звукового сигнала, с',
 recovery_jitter_seconds:'Джиттер восстановления: добавочная пауза, с',expired_seconds:'Истёкшая сессия: паузы попыток, с',server_button_seconds:'Кнопка ошибки: паузы попыток, с',
 server_retry_seconds:'Ошибка сервера без кнопки: паузы, с',blocked_seconds:'Блокировка: паузы новых сессий, с',timeout_retry_seconds:'Клик не отправлен: паузы повторов, с',
 post_timeout_seconds:'Клик отправлен: ожидание результата, с',restart_seconds:'Таймаут действия: пауза перезапуска, с'
};
