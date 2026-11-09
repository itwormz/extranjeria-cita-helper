/** "Защита от бана" fields of the profile editor, in Python 0.3.6 order (`build_appointment_protection`). */
export interface ProtectionField {key:string;title:string;description:string;kind:'flag'|'int';unit?:string;source:'protection'|'timings'}
export const protectionFields:ProtectionField[]=[
 {key:'confirm_new_round',title:'Перепроверять наличие сит',description:'Когда плашка «Сит нет» исчезнет, начнётся новый проход. Запрос уйдёт, если плашка больше не появится.',kind:'flag',source:'protection'},
 {key:'daily_target',title:'Базовый бюджет',description:'Используется без адаптации или пока нет адаптивной оценки. Бюджет только на сегодня можно изменить отдельно.',kind:'int',source:'protection'},
 {key:'adaptive_target',title:'Адаптивный бюджет',description:'Бюджет на завтра определяется числом запросов до первого ответа с cod. oper, но составляет не менее пяти попыток. Запрос с этим ответом не учитывается.',kind:'flag',source:'protection'},
 {key:'retry_after_office',title:'Повторить после офиса',description:'Если после показа офиса сайт ответит «Сит нет», можно сделать одну попытку раньше запланированного времени. Чистый проход и паузы по-прежнему обязательны.',kind:'flag',source:'protection'},
 {key:'minimum_pause_minutes',title:'Пауза между попытками',description:'Минимальное время между отправками запроса на запись, в том числе вне расписания.',kind:'int',unit:'мин',source:'protection'},
 {key:'first_cod_pause_minutes',title:'Пауза после первого cod. oper',description:'Сколько ждать до следующего запроса после первого ответа «Сит нет» с cod. oper.',kind:'int',unit:'мин',source:'protection'},
 {key:'next_cod_pause_minutes',title:'Пауза при повторном cod. oper',description:'Интервал между пробными запросами, пока сайт продолжает отвечать «Сит нет» с cod. oper.',kind:'int',unit:'мин',source:'protection'},
 {key:'adaptive_reload_on_429',title:'Замедлять обновление при 429',description:'Каждый новый HTTP 429 на форме заявителя добавляет 30 секунд к интервалу её обновления до конца запуска.',kind:'flag',source:'timings'},
 {key:'distribute_attempts',title:'Распределять попытки',description:'Распределять бюджет по времени дня. Вне окна чистый проход ждёт; «Попробовать сейчас» открывает ближайшее окно раньше.',kind:'flag',source:'protection'},
];
