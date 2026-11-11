import type {SearchRow} from './bridge';
/** What the journal table shows: single rows, collapsed runs, and the boundary between two passes. */
export type Entry=
 | {kind:'row';id:number;row:SearchRow;rows:SearchRow[]}
 | {kind:'wait';id:number;count:number;first:SearchRow;last:SearchRow;deadline:number|null;rows:SearchRow[]}
 | {kind:'run';id:number;stages:string[];first:SearchRow;last:SearchRow;rows:SearchRow[]}
 | {kind:'pass';id:number;number:number;reason:'site_reset'|'error'|'restart'|null;destination:string|null;time:number;rows:SearchRow[]};
const NO_SLOTS=['applicant_no_slots','request_no_slots','request_no_slots_restart','post_contact_no_slots'];
const ERROR_STAGES=['expired','server_error','blocked','rate_limited'];
const isVisit=(row:SearchRow)=>row.event==='visit';
const isWait=(row:SearchRow)=>isVisit(row)&&NO_SLOTS.includes(row.stage??'')&&['passed','waiting','checked'].includes(row.status)&&(row.destination==null||NO_SLOTS.includes(row.destination));
const isStep=(row:SearchRow)=>isVisit(row)&&row.status==='passed'&&!!row.stage&&!ERROR_STAGES.includes(row.stage)&&!!row.destination&&row.destination!==row.stage&&!isWait(row);
function reasonAfter(previous:SearchRow|null):'site_reset'|'error'|null{
 if(!previous)return null;
 if(previous.status==='unconfirmed'&&previous.destination==='province')return 'site_reset';
 if(ERROR_STAGES.includes(previous.stage??'')||ERROR_STAGES.includes(previous.destination??'')||['interrupted','recovering'].includes(previous.status))return 'error';
 return null;
}
/** Journal entries, newest first. `rows` are in time order (oldest first), as the backend writes them. */
export function buildEntries(rows:SearchRow[]):Entry[]{
 const out:Entry[]=[];
 let previous:SearchRow|null=null;let pass=1;
 const last=()=>out[out.length-1];
 for(const row of rows){
  if(row.event==='start'){out.push({kind:'row',id:row.id,row,rows:[row]});previous=null;pass=1;continue;}
  if(row.event==='flow_restart'){pass+=1;out.push({kind:'pass',id:row.id,number:pass,reason:'restart',destination:null,time:row.time,rows:[row]});previous=null;continue;}
  if(!isVisit(row)){out.push({kind:'row',id:row.id,row,rows:[row]});continue;}
  if(row.stage==='province'&&previous&&previous.stage!=='province'&&previous.stage!=='loading'){
   pass+=1;out.push({kind:'pass',id:-row.id,number:pass,reason:reasonAfter(previous),destination:'province',time:row.time,rows:[]});
  }
  previous=row;
  const top=last();
  if(isWait(row)){
   if(top?.kind==='wait'){top.count+=1;top.last=row;top.deadline=row.deadline??top.deadline;top.rows.push(row);}
   else out.push({kind:'wait',id:row.id,count:1,first:row,last:row,deadline:row.deadline,rows:[row]});
   continue;
  }
  if(isStep(row)){
   if(top?.kind==='run'){top.stages.push(row.destination as string);top.last=row;top.rows.push(row);}
   else if(top?.kind==='row'&&isStep(top.row)&&top.rows.length===1){
    out[out.length-1]={kind:'run',id:top.id,stages:[top.row.stage as string,top.row.destination as string,row.destination as string],first:top.row,last:row,rows:[top.row,row]};
   }
   else out.push({kind:'row',id:row.id,row,rows:[row]});
   continue;
  }
  out.push({kind:'row',id:row.id,row,rows:[row]});
 }
 return out.reverse();
}

export interface DisplayRow{key:string;entryId:number;kind:'row'|'wait'|'run'|'pass'|'child';cells:string[];icon:'alert'|'clock'|'restart'|null;expandable:boolean;open:boolean}
export interface ViewContext{
 time:(ms:number)=>string;minute:(ms:number)=>string;clock:(ms:number)=>string;
 page:(stage:string|null)=>string;what:(row:SearchRow)=>string;t:(key:string)=>string;
 fill:(text:string,values:Record<string,string>)=>string;expanded:number[];
}
const WARN_STATUS=['unconfirmed','interrupted','failed','recovering','review_required','login_required'];
const isWarn=(row:SearchRow)=>WARN_STATUS.includes(row.status)||ERROR_STAGES.includes(row.stage??'');
function range(first:SearchRow,last:SearchRow,ctx:ViewContext){
 if(first.time===last.time)return ctx.time(first.time);
 const a=ctx.minute(first.time),b=ctx.minute(last.time);
 return a===b?ctx.time(first.time):`${a}–${b}`;
}
const stageLabel=(row:SearchRow,ctx:ViewContext)=>row.stage?ctx.page(row.stage):ctx.t('Управление');
/** The rows the table renders for the entries (newest first), with collapsed runs optionally opened. */
export function displayRows(entries:Entry[],ctx:ViewContext):DisplayRow[]{
 const out:DisplayRow[]=[];
 for(const entry of entries){
  const open=ctx.expanded.includes(entry.id);
  if(entry.kind==='pass'){
   const reason=entry.reason==='site_reset'?ctx.fill(ctx.t('сайт вернул на «{destination}»'),{destination:ctx.page(entry.destination)}):entry.reason==='error'?ctx.t('после ошибки сайта'):entry.reason==='restart'?ctx.t('перезапуск'):'';
   const title=ctx.fill(ctx.t('Новый проход {number}'),{number:String(entry.number)});
   out.push({key:`e${entry.id}`,entryId:entry.id,kind:'pass',cells:[ctx.time(entry.time),'',reason?`${title} · ${reason}`:title],icon:'restart',expandable:false,open:false});
  }else if(entry.kind==='wait'){
   // What happened, never when the next check is planned (the timer above the table says that).
   const text=entry.count>1?ctx.fill(ctx.t('Сит нет · проверок: {count}'),{count:String(entry.count)}):ctx.t('Сит нет');
   out.push({key:`e${entry.id}`,entryId:entry.id,kind:'wait',cells:[range(entry.first,entry.last,ctx),stageLabel(entry.last,ctx),text],icon:'clock',expandable:entry.count>1,open});
  }else if(entry.kind==='run'){
   out.push({key:`e${entry.id}`,entryId:entry.id,kind:'run',cells:[range(entry.first,entry.last,ctx),stageLabel(entry.first,ctx),ctx.fill(ctx.t('Шагов: {count} → {destination}'),{count:String(entry.stages.length-1),destination:ctx.page(entry.stages[entry.stages.length-1])})],icon:null,expandable:true,open});
  }else{
   out.push({key:`e${entry.id}`,entryId:entry.id,kind:'row',cells:[ctx.time(entry.row.time),stageLabel(entry.row,ctx),ctx.what(entry.row)],icon:isWarn(entry.row)?'alert':null,expandable:false,open:false});
  }
  if(open&&(entry.kind==='wait'||entry.kind==='run')){
   for(const row of [...entry.rows].reverse())out.push({key:`c${entry.id}-${row.id}`,entryId:entry.id,kind:'child',cells:[ctx.time(row.time),stageLabel(row,ctx),ctx.what(row)],icon:isWarn(row)?'alert':null,expandable:false,open:false});
  }
 }
 return out;
}
