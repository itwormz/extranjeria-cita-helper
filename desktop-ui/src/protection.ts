/** "Защита от бана" fields of the profile editor, in Python 0.3.6 order (`build_appointment_protection`). */
export interface ProtectionField {key:string;title:string;description:string;kind:'flag'|'int';unit?:string;source:'protection'|'timings'}
export const protectionFields:ProtectionField[]=[
 {key:'confirm_new_round',title:'Перепроверять наличие сит',description:'Когда плашка «Сит нет» исчезнет, начнётся новый проход. Запрос уйдёт, если плашка больше не появится.',kind:'flag',source:'protection'},
 {key:'daily_target',title:'Базовый бюджет',description:'Используется без адаптации или пока нет адаптивной оценки. Бюджет только на сегодня можно изменить отдельно.',kind:'int',source:'protection'},
 {key:'adaptive_target',title:'Адаптивный бюджет',description:'Бюджет на завтра определяется числом запросов до первого ответа с cod. oper, но составляет не менее пяти попыток. Запрос с этим ответом не учитывается.',kind:'flag',source:'protection'},
 {key:'retry_after_office',title:'Повторить после офиса',description:'Если после показа офиса сайт ответит «Сит нет», можно сделать одну попытку раньше запланированного времени. Чистый проход и паузы по-прежнему обязательны.',kind:'flag',source:'protection'},
 {key:'adaptive_reload_on_429',title:'Замедлять обновление при 429',description:'Каждый новый HTTP 429 на форме заявителя добавляет 30 секунд к интервалу её обновления до конца запуска.',kind:'flag',source:'timings'},
 {key:'distribute_attempts',title:'Распределять попытки',description:'Распределять бюджет по времени дня. Вне окна чистый проход ждёт; «Попробовать сейчас» открывает ближайшее окно раньше.',kind:'flag',source:'protection'},
];
/**
 * Warning band of the "Проверка сит" section (`update_search_behavior`): entering data on a no-appointments
 * form or switching off any protection flag but "Распределять попытки" is risky. Message 0 = recommended,
 * 1 = repeated submission, 2 = part of the protection is off.
 */
export function searchRisk(enterData:boolean,flags:Record<string,boolean>):{risk:boolean;message:0|1|2}{
 const disabled=Object.entries(flags).some(([key,on])=>key!=='distribute_attempts'&&!on);
 const risk=enterData||disabled;
 return {risk,message:disabled?2:risk?1:0};
}
