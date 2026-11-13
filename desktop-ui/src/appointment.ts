import type {Appointment,AppointmentContext,AppointmentDraft} from './bridge';
/** Resolve edits against the fresh profile context, never an unrelated cached achievement. */
export function recordForEditing<T extends {id:string}>(records:T[],id?:string):T|null{return records.find(record=>record.id===id)??records.at(-1)??null;}
export function maskTime(raw:string):string {const digits=raw.replace(/\D/g,'').slice(0,4);return digits.slice(0,2)+(digits.length>=2?':':'')+digits.slice(2);}
export function timeValue(raw:string):string|null {if(!/^\d{2}:\d{2}$/.test(raw))return null;const [hour,minute]=raw.split(':').map(Number);return hour<24&&minute<60?raw:null;}
export function splitOffice(raw:string):{office:string;address:string}{const comma=raw.indexOf(',');return comma<0?{office:raw.trim(),address:''}:{office:raw.slice(0,comma).trim(),address:raw.slice(comma+1).trim()};}
export function appointmentInput(context:AppointmentContext,saved?:Appointment):AppointmentDraft{
 const value=saved??{date:context.today,time:'09:00',...splitOffice(context.selected_office),confirmation:''};
 const [year,month,day]=value.date.split('-');return {date:`${day}.${month}.${year}`,time:value.time,office:value.office,address:value.address,confirmation:value.confirmation};
}
export function achievementValue(values:Record<string,unknown>,key:string,quality?:{unknown_fields?:string[]}|null):number|null{const value=values[key];if(value===0&&quality?.unknown_fields?.includes(key))return null;return typeof value==='number'&&Number.isFinite(value)&&value>=0?value:null;}
export function searchDays(value:{recorded_at:string;statistics:Record<string,unknown>}):number|null{
 const start=value.statistics.started_at;if(typeof start!=='string')return null;const seconds=Date.parse(value.recorded_at)-Date.parse(start);return Number.isFinite(seconds)?Math.max(0,Math.floor(seconds/86400000)):null;
}
