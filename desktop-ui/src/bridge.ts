import type {BudgetSummary} from './budget';
import type {NotificationDraft,NotificationEditorData} from './notifications';
import type {TimelineRow} from './timeline';
import type {Timings} from './timings';
import {invoke,isTauri} from '@tauri-apps/api/core';
export interface ProfileSummary {id:string;label:string;procedure:string;province:string;valid:boolean}
export interface Preferences {dark:boolean;diagnostics:boolean;sound:boolean;language:string;automatic_updates:boolean}
export interface Bootstrap {preview_source?:string;version:string;profiles:ProfileSummary[];preferences:Preferences;selected_profile:string|null;browser_connected:boolean}
export async function bootstrap():Promise<{data:Bootstrap;native:boolean}>{
 if(isTauri())return {data:await invoke<Bootstrap>('desktop_bootstrap'),native:true};
 return {native:false,data:{version:'0.0.0-dev.1',profiles:[],preferences:{dark:true,diagnostics:false,sound:true,language:'ru',automatic_updates:false},selected_profile:null,browser_connected:false}};
}
export interface ProfileDraft {id:string;label:string;enabled:boolean;procedure:string;province:string;office:string;document_type:string;document_number:string;full_name:string;nationality:string;phone:string;email:string;access_mode:string;clave_method:string;fill_mode:string;no_slots_mode:string;applicant_no_slots_mode:string;subprocedure:string|null;timezone:string;browser_session:string;browser?:string}
export interface AppointmentProtection {confirm_new_round:boolean;daily_target:number;adaptive_target:boolean;distribute_attempts:boolean;retry_after_office:boolean;minimum_pause_minutes:number;first_cod_pause_minutes:number;next_cod_pause_minutes:number}
export interface ProfileEditorData {draft:ProfileDraft;revision:string;timings:Timings;protection:AppointmentProtection;provinces:string[];nationalities:string[]}
export async function loadEditor(id:string|null):Promise<ProfileEditorData>{if(!isTauri())throw new Error('native_required');return invoke('profile_editor',{id});}
export async function saveProfile(id:string|null,draft:ProfileDraft,revision:string,timings:Timings|null=null,notifications:NotificationDraft|null=null,protection:AppointmentProtection|null=null):Promise<{id:string;revision:string}>{if(!isTauri())throw new Error('native_required');return invoke('save_profile',{id,draft,revision,timings,notifications,protection});}

export async function savePreferences(preferences:Preferences):Promise<void>{if(isTauri())await invoke('save_preferences',{preferences});}
export async function selectProfile(id:string):Promise<void>{if(isTauri())await invoke('select_profile',{id});}

/** Explicit lazy operation: bootstrap and the basic profile editor never call it. */
export async function loadNotificationEditor(id:string|null,revision:string):Promise<NotificationEditorData>{if(!isTauri())throw new Error('native_required');return invoke('load_notification_editor',{id,revision});}

export async function notificationAction<T>(request:Record<string,unknown>):Promise<T>{if(!isTauri())throw new Error('native_required');return invoke('notification_action',{request});}

export async function openDiagnostics():Promise<void>{if(!isTauri())throw new Error('native_required');await invoke('open_diagnostics');}

export interface OfficeAppearance {time:string;session_id:string}
export interface OfficeHistory {label:string;count:number|null;last_seen:string|null;appearances:OfficeAppearance[]}
export interface StatisticsDashboard {all:Record<string,unknown>;today:Record<string,unknown>;offices:OfficeHistory[]}
/** Missing history is null; read failures remain errors and never turn into zero totals. */
export async function loadStatistics(id:string):Promise<StatisticsDashboard|null>{if(!isTauri())throw new Error('native_required');return invoke('profile_statistics',{id});}

export type HistoryRange={period:'all'}|{period:'today'}|{period:'range';start:string;end:string};
export interface HistorySnapshot {offices:OfficeHistory[];bounds:{start:string|null;end:string|null};timeline:TimelineRow[]}
export async function loadHistory(id:string,selection:HistoryRange):Promise<HistorySnapshot|null>{if(!isTauri())throw new Error('native_required');return invoke('profile_history',{id,selection});}

export interface SelectionSummary {seconds:number;observed_seconds:number;gap_seconds:number;restored_seconds:number;counts:Record<string,number>;offices:string[];start:string|null;end:string|null}
export async function loadSelection(id:string,selection:HistoryRange,ids:string[]):Promise<SelectionSummary|null>{if(!isTauri())throw new Error('native_required');return invoke('profile_selection',{id,selection,ids});}

export interface AppointmentDraft {date:string;time:string;office:string;address:string;confirmation:string}
export interface Appointment extends AppointmentDraft {timezone:string}
export interface Achievement {statistics_provenance?:{unknown_fields?:string[]}|null;id:string;recorded_at:string;statistics:Record<string,unknown>;appointment:Appointment}
export interface AppointmentContext {records:Achievement[];offices:string[];selected_office:string;today:string}
export async function loadAppointmentContext(id:string):Promise<AppointmentContext>{if(!isTauri())throw new Error('native_required');return invoke('appointment_context',{id});}
export async function saveAppointment(id:string,achievementId:string|null,draft:AppointmentDraft):Promise<Achievement>{if(!isTauri())throw new Error('native_required');return invoke('save_appointment',{id,achievementId,draft});}
export async function copyAppointmentField(id:string,achievementId:string,field:'office'|'address'|'confirmation'):Promise<void>{if(!isTauri())throw new Error('native_required');await invoke('copy_appointment_field',{id,achievementId,field});}

export interface CardBox {op:'box';x:number;y:number;w:number;h:number;color:string;border:string|null;radius:number}
export interface CardTextOp {op:'text';x:number;y:number;w:number;h:number;value:string;size:number;color:string;bold:boolean;right:boolean;wrap:boolean}
export type CardOp=CardBox|CardTextOp
 |{op:'line';x:number;y:number;w:number;color:string}
 |{op:'stroke';color:string;width:number;x1:number;y1:number;x2:number;y2:number}
 |{op:'icon';name:string;x:number;y:number;size:number;color:string}
 |{op:'save'}|{op:'restore'}|{op:'translate';x:number;y:number}|{op:'rotate';angle:number};
export interface CardLayout {width:number;height:number;background:string;ops:CardOp[]}
export interface PublicAchievement {layouts:{dark:CardLayout;light:CardLayout};post:string}
export interface CalendarPreview {date:string;time:string;office:string;address:string;google_url:string}
export async function loadPublicAchievement(id:string,achievementId:string,language:string):Promise<PublicAchievement>{if(!isTauri())throw new Error('native_required');return invoke('achievement_public',{id,achievementId,language});}
export async function loadCalendar(id:string,achievementId:string):Promise<CalendarPreview>{if(!isTauri())throw new Error('native_required');return invoke('achievement_calendar',{id,achievementId});}
export async function achievementExport(id:string,achievementId:string,language:string,request:Record<string,unknown>):Promise<{result?:string}|null>{if(!isTauri())throw new Error('native_required');return invoke('achievement_export',{id,achievementId,language,request});}

export async function openPublicLink(target:'github'|'readme'|'issues'|'sponsor'|'crypto',language:string):Promise<void>{if(!isTauri())throw new Error('native_required');await invoke('open_public_link',{target,language});}

export interface SearchRow {id:number;time:number;stage:string|null;status:string;event:string;destination:string|null;offices:string[];booking_details:{office?:string|null;offers?:{date?:string;time?:string}[]};deadline:number|null}
export interface SearchSnapshot {profile:string|null;active:boolean;paused:boolean;manual:boolean;browser_pid:number|null;page:string|null;deadline:number|null;attention:boolean;error:string|null;warning:string|null;report_id:string|null;rows:SearchRow[];counters:{checks:number};appointment_budget:BudgetSummary|null;revision:number}
export async function searchSnapshot():Promise<SearchSnapshot>{return invoke('search_snapshot');}
export async function startSearch(id:string):Promise<void>{return invoke('search_start',{id});}
export async function controlSearch(command:'pause'|'resume'|'stop'):Promise<void>{return invoke('search_control',{command});}
export interface InstalledBrowser {id:string;name:string;version:string;support:'supported'|'planned'|'unsupported'}
export async function legacyCandidates():Promise<string[]>{return invoke('legacy_candidates');}
export async function legacyImport(name:string):Promise<{copied:number;kept:number}>{return invoke('legacy_import',{name});}
export async function resetSession(id:string):Promise<'reset'|'restarted'>{return invoke('reset_session',{id});}
export async function copyStartupReport(id:string):Promise<void>{return invoke('startup_report_copy',{id});}
export async function loadBrowsers():Promise<InstalledBrowser[]>{return invoke('installed_browsers');}
export async function loadJournalHistory(id:string):Promise<SearchRow[]>{return invoke('journal_history',{id});}
export async function loadBudget(id:string):Promise<BudgetSummary|null>{return invoke('appointment_budget',{id});}
export async function setDailyTarget(target:number|null,id:string|null=null):Promise<void>{return invoke('appointment_daily_target',{id,target});}
export async function setDistributionToday(disabled:boolean):Promise<void>{return invoke('appointment_distribution',{disabled});}
export async function tryAppointmentNow():Promise<void>{return invoke('appointment_try_now');}
export async function resumeSearch():Promise<void>{return invoke('search_resume');}
export async function showSearchBrowser():Promise<void>{return invoke('search_show');}
export async function copyText(text:string):Promise<void>{return invoke('copy_text',{text});}
