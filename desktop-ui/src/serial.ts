/** One queue for all profile/preference writes; readers can await a consistent revision. */
export class SerialWrites {
 #tail:Promise<unknown>=Promise.resolve();
 enqueue<T>(work:()=>Promise<T>):Promise<T>{const task=this.#tail.catch(()=>{}).then(work);this.#tail=task;return task;}
 settled():Promise<void>{return this.#tail.then(()=>{},()=>{});}
}
