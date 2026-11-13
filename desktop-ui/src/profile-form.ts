/** Editing a text field must not activate the form's default Save button. */
export function preventImplicitProfileSubmit(event:{key:string;isComposing:boolean;target:EventTarget|null;preventDefault:()=>void}):void{
 const tag=(event.target as HTMLElement|null)?.tagName;
 if(event.key==='Enter'&&!event.isComposing&&(tag==='INPUT'||tag==='SELECT'))event.preventDefault();
}
export function guardProfileSubmit(form:HTMLFormElement):{destroy:()=>void}{
 form.addEventListener('keydown',preventImplicitProfileSubmit);
 return {destroy:()=>form.removeEventListener('keydown',preventImplicitProfileSubmit)};
}
