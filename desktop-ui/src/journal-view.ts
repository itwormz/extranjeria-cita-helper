import type {SearchRow} from './bridge';
/** What the journal shows: only things that happened. Navigation steps are not events and are left out. */
export type Entry=
 | {kind:'row';id:number;row:SearchRow}
 | {kind:'wait';id:number;count:number;first:SearchRow;last:SearchRow}
 | {kind:'pass';id:number;number:number;reason:'restart'|null;time:number};
const NO_SLOTS=['applicant_no_slots','request_no_slots','request_no_slots_restart','post_contact_no_slots'];
const ERROR_STAGES=['expired','server_error','blocked','rate_limited'];
const isVisit=(row:SearchRow)=>row.event==='visit';
const isWait=(row:SearchRow)=>isVisit(row)&&NO_SLOTS.includes(row.stage??'')&&['passed','waiting','checked'].includes(row.status)&&(row.destination==null||NO_SLOTS.includes(row.destination));
/** A visit row that is only a step of the way or still in progress: not worth a line. */
function isNoise(row:SearchRow):boolean{
 if(row.status==='processing'||row.status==='recovering')return true;
 if(row.status==='passed')return !row.selected_office;
 if(row.status==='waiting')return !ERROR_STAGES.includes(row.stage??'');
 return false;
}
/** Journal entries, newest first. `rows` are in time order (oldest first), as the backend writes them. */
export function buildEntries(rows:SearchRow[]):Entry[]{
 const out:Entry[]=[];
 let previous:SearchRow|null=null;let pass=1;
 for(const row of rows){
  if(row.event==='start'){out.push({kind:'row',id:row.id,row});previous=null;pass=1;continue;}
  if(row.event==='flow_start')continue;
  if(row.event==='flow_restart'){pass+=1;out.push({kind:'pass',id:row.id,number:pass,reason:'restart',time:row.time});previous=null;continue;}
  if(!isVisit(row)){out.push({kind:'row',id:row.id,row});continue;}
  // The site (or an error) sent the bot back to the first page: a new pass begins.
  if(row.stage==='province'&&previous&&previous.stage!=='province'&&previous.stage!=='loading'){
   pass+=1;out.push({kind:'pass',id:-row.id,number:pass,reason:null,time:row.time});
  }
  previous=row;
  if(isWait(row)){
   const top=out[out.length-1];
   if(top?.kind==='wait'){top.count+=1;top.last=row;}
   else out.push({kind:'wait',id:row.id,count:1,first:row,last:row});
   continue;
  }
  if(isNoise(row))continue;
  out.push({kind:'row',id:row.id,row});
 }
 return out.reverse();
}

export interface DisplayRow{key:string;kind:'row'|'wait'|'pass';cells:[string,string];icon:'alert'|'clock'|'restart'|'office'|null}
export interface ViewContext{
 time:(ms:number)=>string;minute:(ms:number)=>string;what:(row:SearchRow)=>string;
 t:(key:string)=>string;fill:(text:string,values:Record<string,string>)=>string;
}
const WARN_STATUS=['unconfirmed','interrupted','failed','review_required','login_required'];
const isWarn=(row:SearchRow)=>WARN_STATUS.includes(row.status)||ERROR_STAGES.includes(row.stage??'');
function range(first:SearchRow,last:SearchRow,ctx:ViewContext){
 if(first.time===last.time)return ctx.time(first.time);
 const a=ctx.minute(first.time),b=ctx.minute(last.time);
 return a===b?ctx.time(first.time):`${a}–${b}`;
}
/** The rows the table renders for the entries (newest first): a time and what happened. */
export function displayRows(entries:Entry[],ctx:ViewContext):DisplayRow[]{
 return entries.map((entry):DisplayRow=>{
  if(entry.kind==='pass'){
   const title=ctx.fill(ctx.t('Новый проход {number}'),{number:String(entry.number)});
   return {key:`e${entry.id}`,kind:'pass',cells:[ctx.time(entry.time),entry.reason==='restart'?`${title} · ${ctx.t('перезапуск')}`:title],icon:'restart'};
  }
  if(entry.kind==='wait'){
   return {key:`e${entry.id}`,kind:'wait',cells:[range(entry.first,entry.last,ctx),entry.count>1?ctx.fill(ctx.t('Сит нет · проверок: {count}'),{count:String(entry.count)}):ctx.t('Сит нет')],icon:'clock'};
  }
  const row=entry.row;
  return {key:`e${entry.id}`,kind:'row',cells:[ctx.time(row.time),ctx.what(row)],icon:row.status==='offices_found'?'office':isWarn(row)?'alert':null};
 });
}
