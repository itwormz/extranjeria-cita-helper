import type {SerialWrites} from './serial';
export interface PersistedResult<T>{generation:number;ok:boolean;value:T}
/** Rollback follows confirmed writes, independently of later optimistic UI edits. */
export class PersistedValue<T>{
 private confirmed:T;private generation=0;private persist:(value:T)=>Promise<void>;private queue:SerialWrites;
 constructor(initial:T,persist:(value:T)=>Promise<void>,queue:SerialWrites){this.confirmed=structuredClone(initial);this.persist=persist;this.queue=queue;}
 isCurrent(result:PersistedResult<T>):boolean{return result.generation===this.generation;}
 set(value:T):Promise<PersistedResult<T>>{const snapshot=structuredClone(value);const generation=++this.generation;return this.queue.enqueue(async()=>{let ok=false;try{await this.persist(snapshot);this.confirmed=snapshot;ok=true;}catch{/* Caller gets a safe status; backend details never enter UI state. */}return {generation,ok,value:structuredClone(this.confirmed)};});}
}
