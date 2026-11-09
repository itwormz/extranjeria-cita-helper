step => {
 let started=false;
 try {
  // A same-URL assignment makes a fresh GET, never reuses the certificate POST.
  if(step.fetchApplicant){
   if(location.pathname.split('/').pop()!=='acEntrada')return 'refused';
   started=true;location.assign(location.href);
  }else{started=true;location.reload();}
  return 'changed';
 }catch(_){return started ? 'unknown' : 'refused';}
}
