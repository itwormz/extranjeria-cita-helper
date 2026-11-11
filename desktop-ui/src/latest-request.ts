export interface LatestResult<T>{generation:number;ok:boolean;value?:T}
/** Serialize checks but only publish the result of the latest requested channel. */
export class LatestRequest<T>{
 private generation=0;private tail:Promise<unknown>=Promise.resolve();
 invalidate(){this.generation++;}
 isCurrent(result:LatestResult<T>){return result.generation===this.generation;}
 run(work:()=>Promise<T>):Promise<LatestResult<T>>{
  const generation=++this.generation;
  const result=this.tail.catch(()=>{}).then(async()=>{
   if(generation!==this.generation)return {generation,ok:false};
   try{return {generation,ok:true,value:await work()};}catch{return {generation,ok:false};}
  });
  this.tail=result;return result;
 }
}
