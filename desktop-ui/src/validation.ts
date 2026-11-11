/** Convert backend validation codes to user-facing messages; never echo unknown payloads. */
export function profileErrors(errors:string[],t:(key:string)=>string):string[]{
 const rules:[string,string][]=[['label','Указать название профиля'],['applicant.contact.phone','Телефон: от 6 до 15 цифр'],['applicant.contact.email','Указать корректную почту'],['request.province','Выбрать провинцию'],['applicant.nationality','Выбрать гражданство'],['applicant.full_name','Имя и фамилия: 1–50 символов, без цифр'],['applicant.document_number','Проверить номер документа'],['applicant.document_type','Выбрать тип документа'],['request.procedure','Выбрать услугу'],['request.access_mode','Выбрать способ авторизации'],['request.clave_method','Выбрать способ авторизации'],['request.no_slots_mode','Выбрать способ повторной проверки'],['request.applicant_no_slots_mode','Выбрать поведение на форме заявителя'],['schedule.timezone','Выбрать корректный часовой пояс']];
 return [...new Set(errors.map(error=>t(error==='Invalid profile timings'?'Проверить значения интервалов':error==='Invalid notification settings'?'Проверить настройки уведомлений':rules.find(([prefix])=>error.startsWith(`${prefix} `)||error.startsWith(`${prefix}:`))?.[1]??'Проверить данные профиля')))];
}

export interface Protection {confirm_new_round:boolean;daily_target:number;adaptive_target:boolean;distribute_attempts:boolean;retry_after_office:boolean}
/** Python `AppointmentProtection()` defaults. */
export const defaultProtection:Protection={confirm_new_round:true,daily_target:5,adaptive_target:true,distribute_attempts:false,retry_after_office:true};
const integerKeys=['daily_target'] as const;
const flagKeys=['confirm_new_round','adaptive_target','distribute_attempts','retry_after_office'] as const;
/** Keys with an invalid value: integers must be 1…2147483647, flags must be booleans. */
export function validateProtection(value:Record<string,unknown>):string[]{
 const bad:string[]=[];
 for(const key of integerKeys){const v=value[key];if(typeof v!=='number'||!Number.isInteger(v)||v<1||v>2147483647)bad.push(key);}
 for(const key of flagKeys)if(typeof value[key]!=='boolean')bad.push(key);
 return bad;
}
