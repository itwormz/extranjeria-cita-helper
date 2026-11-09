export function dateISO(value:string):string|null {
 if(!/^\d{2}\.\d{2}\.\d{4}$/.test(value))return null;
 const [day,month,year]=value.split('.').map(Number);if(year<1||year>=9999)return null;
 const date=new Date(0);date.setUTCFullYear(year,month-1,day);date.setUTCHours(0,0,0,0);
 return date.getUTCFullYear()===year&&date.getUTCMonth()===month-1&&date.getUTCDate()===day?`${String(year).padStart(4,'0')}-${String(month).padStart(2,'0')}-${String(day).padStart(2,'0')}`:null;
}
export function dateText(iso:string):string {const [year,month,day]=iso.split('-');return `${day}.${month}.${year}`;}
export function maskDate(raw:string):string {const digits=raw.replace(/\D/g,'').slice(0,8);return digits.slice(0,2)+(digits.length>=2?'.':'')+digits.slice(2,4)+(digits.length>=4?'.':'')+digits.slice(4);}
export function monthDays(month:string):string[]{const start=new Date(`${month}T12:00:00Z`);const offset=(start.getUTCDay()+6)%7;return Array.from({length:42},(_,index)=>{const day=new Date(start);day.setUTCDate(day.getUTCDate()-offset+index);return day.toISOString().slice(0,10);});}
export function moveMonth(month:string,delta:number):string {const date=new Date(`${month}T12:00:00Z`);date.setUTCMonth(date.getUTCMonth()+delta);return date.toISOString().slice(0,10);}
export function todayISO():string {return new Intl.DateTimeFormat('sv-SE',{timeZone:'Europe/Madrid',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date());}
