import proverbs from './proverbs.json' with {type:'json'};
export interface Point {x:number;y:number}
export function escapePoint(width:number,height:number,cursor:Point,random= Math.random):Point{
 const maxX=Math.max(0,width-56),maxY=Math.max(0,height-56);
 const points=Array.from({length:16},()=>({x:random()*maxX,y:random()*maxY}));
 return points.reduce((best,p)=>Math.abs(p.x+28-cursor.x)+Math.abs(p.y+28-cursor.y)>Math.abs(best.x+28-cursor.x)+Math.abs(best.y+28-cursor.y)?p:best,points[0]);
}
export function proverb(language:string,random=Math.random):string{
 const items=proverbs[language as keyof typeof proverbs]??proverbs.en;
 return items[Math.min(items.length-1,Math.max(0,Math.floor(random()*items.length)))];
}
