// Editable values only. Vault references never cross this UI boundary.
export interface Recipient {id:string;label:string;type:'private'|'group'|'supergroup'|'channel';username:string}
export type NotificationEvent='auth'|'office'|'booking'|'error';
export interface NamedValue {name:string;value:string;secret:boolean}
export interface WebhookConfig {url:string;method:'GET'|'POST';headers:NamedValue[];query:NamedValue[];body_mode:'standard'|'custom';body:string}
export interface NotificationDraft {telegram:{enabled:boolean;events:NotificationEvent[];recipients:Recipient[];token:string};webhook:{enabled:boolean;events:NotificationEvent[];config:WebhookConfig}}
export interface NotificationEditorData {revision:string;draft:NotificationDraft}
export interface PairTicket {id:string;link:string;expires_in_seconds:number}
export function pairingLinkHint(seconds:number|undefined,t:(key:string)=>string):string{
 return typeof seconds==='number'&&Number.isFinite(seconds)&&seconds>0
  ?t('Для другого получателя — отправить ему ссылку. Срок действия: {minutes} мин.').replace('{minutes}',String(Math.ceil(seconds/60)))
  :t('Для другого получателя — отправить ему ссылку.');
}

export function notificationError(error:unknown,t:(key:string)=>string):string{
 const failure=error as {code?:string;status?:number};
 const labels:Record<string,string>={invalid_config:'Проверить настройки уведомлений',invalid_response:'Не удалось распознать ответ',network:'Не удалось подключиться',existing_webhook:'У бота уже настроен webhook. Использовать ручной Chat ID.',pair_busy:'Подключение этого бота уже открыто в другом окне.',pair_expired:'Ссылка истекла. Повторить подключение.',unavailable:'Действие недоступно',secrets_locked:'Хранилище секретов заблокировано',secrets_missing:'Сохранённые секреты не найдены',secrets_unavailable:'Не удалось открыть хранилище секретов',conflict:'Файл изменён извне. Перед сохранением открыть настройки заново.'};
 return failure?.code==='http'?`${t('Ошибка HTTP')} ${failure.status??'—'}`:t(labels[failure?.code??'']??'Не удалось выполнить действие');
}
