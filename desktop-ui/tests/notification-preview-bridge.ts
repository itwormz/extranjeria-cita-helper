import {translate} from '../src/i18n';
import timelineFixture from './statistics-preview-data.json';
// Visual fixture only: no imports of Tauri, HTTP transports or credential stores.
export async function notificationAction<T>(request:Record<string,unknown>):Promise<T>{
 const recipient={id:'123456',label:'Fictitious Person',type:'private',username:'fixture_person'};
 const values:Record<string,unknown>={check_bot:{username:'fixture_bot'},begin_pair:{id:'fixture-pair',link:'https://t.me/fixture_bot?start=FIXTURE'},poll_pair:recipient,chat:recipient,telegram_test:{elapsed_ms:15},webhook_test:{status:204,elapsed_ms:20},validate_webhook:null,cancel_pair:null,open_pair:null,copy_pair:null};
 if(!(String(request.action) in values))throw {code:'invalid_config'};
 return structuredClone(values[String(request.action)]) as T;
}

let statisticsScenario='full';
export function setStatisticsScenario(value:string){statisticsScenario=value;}
export async function loadStatistics(id:string){
 if(statisticsScenario==='error')throw new Error('PRIVATE_ERROR_NOT_FOR_UI');
 if(statisticsScenario==='empty')return null;
 const legacy=statisticsScenario==='legacy';const base={checks:37588,active_seconds:41400,days:3,offices_seen:18,unique_offices:10,booking_screens:4,successes:1,server_errors:61,rate_limited:2,blocked:8,flow_passes:205,actions_performed:61200,pages_processed:48200,effort_started_at:'2026-10-20T09:00:00Z',first_office_at:'2026-10-20T10:12:13Z',first_office_seconds:8040,first_office_status:'known',first_office_active_quality:'complete',first_booking_at:'2026-10-21T10:12:13Z',first_booking_seconds:18480,first_booking_status:'recorded',first_booking_active_quality:'minimum',legacy_unknown_fields:[]};
 const all=legacy?{...base,active_seconds:0,days:0,actions_performed:0,pages_processed:0,effort_started_at:null,legacy_unknown_fields:['active_seconds','started_at','actions_performed','pages_processed']}:base;
 return {all,today:{...base,checks:123,active_seconds:7500,days:1,successes:0,first_booking_status:'not_seen',first_booking_at:null,first_booking_seconds:null},offices:[]};
}

export async function loadHistory(id:string,selection:{period:string;start?:string;end?:string}){
 const days=['2026-10-20T10:00:00Z','2026-10-20T11:00:00Z','2026-10-21T10:00:00Z','2026-10-22T10:00:00Z','2026-10-23T10:00:00Z','2026-10-24T10:00:00Z'];
 const selected=days.filter(day=>selection.period!=='range'||day.slice(0,10)>=(selection.start??'')&&day.slice(0,10)<=(selection.end??''));
 const offices=selected.length?[{label:'CNP COMISARIA CASTELLDEFELS, PLAÇA DE L’ESPERANTO, 4, CASTELLDEFELS',count:selected.length,last_seen:selected.at(-1),appearances:selected.map(time=>({time,session_id:'fixture'}))}]:[];
 if(selection.period==='all')offices.push({label:'Fixture historical office',count:null,last_seen:null,appearances:[]} as any);
 return {bounds:{start:days[0],end:days.at(-1)},timeline:timelineFixture,offices};
}

export async function loadSelection(id:string,selection:unknown,ids:string[]){
 const rows=timelineFixture.filter(row=>ids.includes(`${row.event?'event':'clip'}:${row.id}`));const starts=rows.map(row=>Date.parse(row.start));const ends=rows.map(row=>Date.parse(row.end));const seconds=rows.filter(row=>!row.event).reduce((sum,row)=>sum+(Date.parse(row.end)-Date.parse(row.start))/1000,0);
 return {start:starts.length?new Date(Math.min(...starts)).toISOString():null,end:ends.length?new Date(Math.max(...ends)).toISOString():null,seconds,observed_seconds:seconds,gap_seconds:0,restored_seconds:0,counts:{checks:42,actions_performed:128,pages_processed:64,offices_seen:2,booking_screens:1},offices:['Fixture office']};
}

let appointmentRecords:any[]=[];
export async function loadAppointmentContext(id:string){return {records:appointmentRecords,offices:['CNP COMISARIA CASTELLDEFELS, PLAÇA DE L’ESPERANTO, 4, CASTELLDEFELS','CNP COMISARIA GRANOLLERS, RICOMA, 65, GRANOLLERS'],selected_office:'CNP COMISARIA GRANOLLERS, RICOMA, 65, GRANOLLERS',today:'2026-10-31'};}
export async function saveAppointment(id:string,achievementId:string|null,draft:unknown){let value=appointmentRecords.find(row=>row.id===achievementId);if(value)value={...value,appointment:{...(draft as object),timezone:'Europe/Madrid'}};else value={id:'PRIVATE-APPOINTMENT-FIXTURE',recorded_at:'2026-10-31T15:00:00Z',statistics:{checks:37033,active_seconds:41520,started_at:'2026-10-14T15:00:00Z',offices_seen:18,unique_offices:10,booking_screens:4,flow_passes:205,server_errors:61,rate_limited:0,blocked:8,first_available_seconds:3600,actions_performed:1280,pages_processed:640},appointment:{...(draft as object),timezone:'Europe/Madrid'}};appointmentRecords=appointmentRecords.filter(row=>row.id!==value.id).concat([value]);return value;}
export async function copyAppointmentField(){return;}

export async function loadPublicAchievement(id:string,key:string,language:string){const r=appointmentRecords.find(row=>row.id===key);const t=(key:string)=>translate(key,language);const stats=r?.statistics??{};const rows=[['Проверок сит','checks'],['Активный поиск','active_seconds'],['Появлений офисов','offices_seen'],['Разных офисов','unique_offices'],['Появления выбора даты','booking_screens'],['Первый офис','first_available_seconds'],['Ошибок сервера','server_errors'],['Ограничений HTTP 429','rate_limited'],['Ограничения доступа','blocked'],['Повторные проходы','flow_passes'],['Действий выполнено','actions_performed'],['Страниц обработано','pages_processed']].map(([label,key])=>[t(label),key==='active_seconds'?'11 h 32 min':key==='first_available_seconds'?'1 h':String(stats[key]??0)]);return {card:{title:t('Сита получена'),duration:'17 '+(language==='ru'?'дней поиска':language==='es'?'días de búsqueda':'days of searching'),rows,credit:t('Получено с помощью Cita Extranjeria Helper'),width:1200,height:900,top_strip_height:7},post:t('Сита получена')+'!\n'+rows.map(([label,value])=>label+': '+value).join('\n')+'\n'+t('Получено с помощью Cita Extranjeria Helper')};}
export async function loadCalendar(id:string,key:string){const r=appointmentRecords.find(row=>row.id===key);return {...r?.appointment,google_url:'https://calendar.google.com/calendar/render'};}
export async function achievementExport(id:string,key:string,language:string,request:any){return {result:({google:'google_opened',save_calendar:'calendar_saved',save_image:'image_saved',copy_image:'image_copied',prepare_text:'text_prepared',prepare_image:'image_prepared'} as any)[request.action]??''};}
export async function openPublicLink(){return;}
