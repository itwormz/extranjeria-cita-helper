import type {SearchRow} from './bridge';
/** "What happened" of a log row, as Python's page journal words it: a template and its values. */
export interface JournalText{template:string;values:Record<string,string>}
const ERROR_STAGES=['expired','server_error','blocked','rate_limited'];
const NO_SLOTS=['request_no_slots','request_no_slots_restart','post_contact_no_slots'];
const ACTION:Record<string,string>={province:'Выбираем провинцию',procedure:'Выбираем услугу',auth:'Начинаем авторизацию',auth_wait:'Открываем выбранный способ авторизации',applicant:'Заполняем данные заявителя',request_menu:'Запрашиваем ситу',contact:'Заполняем контактные данные',office:'Выбираем офис',request_no_slots:'Повторяем проверку сит',applicant_no_slots:'Обновляем форму без ввода данных',request_no_slots_restart:'Возвращаемся к началу через Aceptar',post_contact_no_slots:'Завершаем проход после отсутствия сит'};
const SENT:Record<string,string>={province:'Выбор провинции отправлен — ожидается ответ',procedure:'Выбор услуги отправлен — ожидается ответ',applicant:'Данные заявителя отправлены — ожидается ответ',contact:'Контактные данные отправлены — ожидается ответ',office:'Выбранный офис отправлен — ожидается ответ'};
const PASSED:Record<string,string>={province:'→ {destination}',procedure:'→ {destination}',auth:'→ {destination}',auth_wait:'Вход выполнен → {destination}',applicant:'→ {destination}',contact:'→ {destination}',office:'Выбран офис: {selected_office} → {destination}'};
const CONTROL:Record<string,string>={start:'Поиск запущен',finish:'Поиск завершён',pause:'Автоматизация приостановлена',resume:'Обработка продолжается',flow_start:'Начинаем первый проход',flow_restart:'Начинаем новый проход',handoff:'Автоматизация остановлена — браузер доступен для ручного оформления',statistics_error:'Не удалось сохранить счётчик профиля',browser_shown:'Браузер показан',browser_show_failed:'Не удалось показать браузер'};
const OTHER=['Распознана страница — определяем следующий шаг','Обрабатываем сообщение об ошибке','Неизвестная страница — требуется ручная проверка','Открыт выбор даты и времени — управление передано пользователю','Действие отправлено — ожидается ответ','Запрос проверки сит отправлен — ожидается ответ','Ошибка сохраняется — требуется следующая попытка','Ошибка сайта → {destination}','Ответ не распознан → {destination}','→ {destination} (после ошибки)','Доступ восстановлен → {destination}','Сит нет','→ {destination}','Другая страница → {destination}','Страница обновилась','Доступен офис: {offices}','Доступно несколько офисов: {offices} — требуется выбор','Открыта авторизация — требуется выполнить вход','Требуется ручная проверка страницы','Восстанавливаем доступ — ожидается результат','Сит нет · следующая проверка в {deadline}','Сит нет · новый проход в {deadline}','Следующая попытка в {deadline}','Работа прервана пользователем','Поиск завершён до подтверждения перехода','Действие прервано ошибкой — работа завершена','Обработка продолжается','Выбран офис: {selected_office} → {destination}'];
/** Every template the journal can produce: each needs a translation. */
export const journalTemplates:string[]=[...new Set([...Object.values(ACTION),...Object.values(SENT),...Object.values(PASSED),...Object.values(CONTROL),...OTHER])];
const authFinished=(row:SearchRow)=>row.stage==='auth'&&row.destination!=='auth_wait';
export function journalMessage(row:SearchRow):JournalText{
 const values:Record<string,string>={destination:row.destination??'',offices:row.offices.join(' · '),selected_office:row.selected_office??'',deadline:''};
 const text=(template:string):JournalText=>({template,values});
 if(row.event!=='visit'){
  if(row.event==='finish'&&row.status==='cancelled')return text('Работа прервана пользователем');
  if(row.event==='finish'&&row.status==='interrupted')return text('Поиск завершён до подтверждения перехода');
  if(row.event==='fatal_error')return text('Действие прервано ошибкой — работа завершена');
  return text(CONTROL[row.event]??'Распознана страница — определяем следующий шаг');
 }
 const stage=row.stage??'';
 const action=stage==='applicant_no_slots'&&row.applicant_continue?'applicant':stage;
 switch(row.status){
  case 'unconfirmed':return text(row.destination==='unknown'?'Ответ не распознан → {destination}':row.destination===stage?'Страница обновилась':'Другая страница → {destination}');
  case 'interrupted':return text(ERROR_STAGES.includes(row.destination??'')?'Ошибка сайта → {destination}':'Поиск завершён до подтверждения перехода');
  case 'passed':return text(ERROR_STAGES.includes(stage)?(stage==='rate_limited'?'Доступ восстановлен → {destination}':'→ {destination} (после ошибки)'):authFinished(row)?'Вход выполнен → {destination}':PASSED[action]??'→ {destination}');
  case 'checked':return text('Сит нет');
  case 'waiting':return text(ERROR_STAGES.includes(stage)?'Ошибка сохраняется — требуется следующая попытка':stage==='request_no_slots'?'Сит нет · следующая проверка в {deadline}':NO_SLOTS.includes(stage)?'Сит нет · новый проход в {deadline}':'Следующая попытка в {deadline}');
  case 'offices_found':return text('Доступен офис: {offices}');
  case 'choice_required':return text('Доступно несколько офисов: {offices} — требуется выбор');
  case 'login_required':return text('Открыта авторизация — требуется выполнить вход');
  case 'review_required':return text(stage==='unknown'?'Неизвестная страница — требуется ручная проверка':'Требуется ручная проверка страницы');
  case 'date_available':return text('Открыт выбор даты и времени — управление передано пользователю');
  case 'recovering':return text(ERROR_STAGES.includes(stage)?'Обрабатываем сообщение об ошибке':'Восстанавливаем доступ — ожидается результат');
  case 'paused':return text('Автоматизация приостановлена');
  case 'cancelled':return text('Работа прервана пользователем');
  case 'failed':return text('Действие прервано ошибкой — работа завершена');
  default:
   if(row.checked&&row.submitted)return text('Запрос проверки сит отправлен — ожидается ответ');
   if(row.submitted)return text(SENT[action]??'Действие отправлено — ожидается ответ');
   return text(ACTION[action]??'Распознана страница — определяем следующий шаг');
 }
}
