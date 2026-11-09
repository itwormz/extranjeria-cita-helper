/** Opening a saved file may succeed without a payload; keep the last useful feedback. */
export function exportFeedback(value:unknown,previous=''):string{
 if(value&&typeof value==='object'&&'result' in value&&typeof value.result==='string'&&value.result!=='cancelled')return value.result;
 return previous;
}
