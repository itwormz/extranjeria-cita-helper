export type Timings=Record<string,number|number[]>;
export type TimingInputs=Record<string,string[]>;
export const sequences=new Set(['expired_seconds','server_button_seconds','server_retry_seconds','blocked_seconds','timeout_retry_seconds']);
export function timingInputs(values:Timings):TimingInputs{return Object.fromEntries(Object.entries(values).map(([key,value])=>[key,Array.isArray(value)?sequences.has(key)?[value.join(', ')]:value.map(String):[String(value)]]));}
export function parseTimingInputs(fields:TimingInputs,shape:Timings):Timings{
 return Object.fromEntries(Object.entries(shape).map(([key,value])=>{
  const parts=sequences.has(key)?fields[key]?.[0]?.split(','):fields[key];
  if(!parts?.length)throw new Error(key);
  const numbers=parts.map(part=>{const normalized=part.trim().replace(',','.');if(!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(normalized))throw new Error(key);const number=Number(normalized);if(!Number.isFinite(number))throw new Error(key);return number;});
  if(!sequences.has(key)&&numbers.length!==(Array.isArray(value)?2:1))throw new Error(key);
  return [key,Array.isArray(value)?numbers:numbers[0]];
 }));
}
export const timingGroups=[
 {title:'Ввод данных',fields:[['action_delay_seconds','Между действиями'],['typing_ms','Ввод NIE и имени'],['contact_typing_ms','Ввод контактов']]},
 {title:'Проверка сит',fields:[['reload_seconds','Интервал проверки'],['navigation_seconds','Таймаут страницы'],['bootstrap_seconds','Начальная загрузка / F5']]},
 {title:'Восстановление',fields:[['no_slots_restart_seconds','Новый проход · Aceptar'],['clave_error_seconds','Ошибка Cl@ve'],['rate_limit_seconds','HTTP 429'],['recovery_jitter_seconds','Добавочная пауза'],['expired_seconds','Истёкшая сессия'],['server_button_seconds','Кнопка ошибки'],['server_retry_seconds','Ошибка без кнопки'],['blocked_seconds','Блокировка'],['timeout_retry_seconds','Клик не отправлен'],['post_timeout_seconds','Ожидание после клика'],['restart_seconds','Перезапуск после таймаута']]},
 {title:'Уведомления',fields:[['alert_seconds','Повтор звонка']]}
];
