import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
import {drawCard,fitText,wrapLines} from '../src/card-draw.ts';
const fixture=JSON.parse(readFileSync(new URL('../../tests/contracts/fixtures/achievement-card-0.3.6.json',import.meta.url),'utf8'));
/** Recording canvas stand-in: every character is half a pixel-size wide. */
function fake(){const calls=[];const ctx={font:'400 24px Plex',fillStyle:'',strokeStyle:'',lineWidth:1,textAlign:'left',textBaseline:'alphabetic',
 measureText(text){const size=Number(/(\d+)px/.exec(this.font)[1]);return {width:[...text].length*size/2};}};
 for(const name of ['fillRect','beginPath','moveTo','lineTo','arcTo','closePath','fill','stroke','save','restore','translate','rotate','fillText'])ctx[name]=(...args)=>{calls.push([name,...args,{fill:ctx.fillStyle,stroke:ctx.strokeStyle,width:ctx.lineWidth,align:ctx.textAlign}]);};
 return {ctx,calls};}
const text=(extra={})=>({op:'text',x:10,y:100,w:200,h:60,value:'abc',size:24,color:'#111',bold:false,right:false,wrap:false,...extra});

test('text keeps its size when it fits and shrinks one pixel at a time to the 18 px floor',()=>{
 const {ctx}=fake();
 assert.equal(fitText(ctx,text()).size,24);
 assert.equal(fitText(ctx,text({value:'x'.repeat(20)})).size,20);
 assert.equal(fitText(ctx,text({value:'x'.repeat(200)})).size,18);
});
test('wrapped text breaks on spaces and must also fit the box height',()=>{
 const {ctx}=fake();ctx.font='400 20px Plex';
 assert.deepEqual(wrapLines(ctx,'one two three',100),['one two','three']);
 assert.deepEqual(wrapLines(ctx,'unbreakable',20),['unbreakable']);
 const fitted=fitText(ctx,text({value:'one two three four five six',w:100,h:60,size:22,wrap:true}));
 assert.ok(fitted.lines.length*fitted.size*1.3<=60||fitted.size===18);
 assert.ok(fitted.size<22);
});
test('right-aligned values end at the right edge and text is centred vertically',()=>{
 const {ctx,calls}=fake();
 return drawCard(ctx,{width:1200,height:1240,background:'#000',ops:[text({right:true,x:500,w:80,y:100,h:50,value:'4'})]},()=>{}).then(()=>{
  const [,value,x,y,state]=calls.find(call=>call[0]==='fillText');
  assert.equal(value,'4');assert.equal(x,580);assert.equal(y,125);assert.equal(state.align,'right');
 });
});
test('rotation is converted from the Python degrees and states are balanced',async()=>{
 const {ctx,calls}=fake();
 await drawCard(ctx,{width:10,height:10,background:'#000',ops:[{op:'save'},{op:'translate',x:85,y:74},{op:'rotate',angle:-25},{op:'restore'}]},()=>{});
 assert.deepEqual(calls.map(call=>call[0]),['fillRect','save','translate','rotate','restore']);
 assert.ok(Math.abs(calls[3][1]+25*Math.PI/180)<1e-12);
});
test('boxes fill, optionally border, and lines and strokes keep their own colour and width',async()=>{
 const {ctx,calls}=fake();
 await drawCard(ctx,{width:10,height:10,background:'#000',ops:[
  {op:'box',x:0,y:0,w:100,h:50,color:'#222',border:'#333',radius:10},{op:'box',x:0,y:0,w:6,h:6,color:'#444',border:null,radius:1},
  {op:'line',x:5,y:7,w:20,color:'#555'},{op:'stroke',color:'#666',width:2,x1:1,y1:2,x2:3,y2:4}]},()=>{});
 const strokes=calls.filter(call=>call[0]==='stroke');
 assert.equal(strokes.length,3);
 assert.deepEqual(strokes.map(call=>[call.at(-1).stroke,call.at(-1).width]),[['#333',1],['#555',1],['#666',2]]);
 assert.equal(calls.filter(call=>call[0]==='fill').length,2);
});
test('every Python reference card replays completely: icons painted in order, nothing unknown',async()=>{
 for(const item of fixture.cases){
  const {ctx,calls}=fake();const icons=[];
  await drawCard(ctx,{width:item.size[0],height:item.size[1],background:item.background,ops:item.ops},(...args)=>{icons.push(args);});
  assert.deepEqual(icons.map(icon=>icon[0]),item.ops.filter(op=>op.op==='icon').map(op=>op.name),item.id);
  assert.equal(calls.filter(call=>call[0]==='save').length,calls.filter(call=>call[0]==='restore').length,item.id);
  assert.equal(calls.filter(call=>call[0]==='fillText').length>=item.ops.filter(op=>op.op==='text').length,true,item.id);
  assert.deepEqual(calls[0].slice(0,5),['fillRect',0,0,1200,1240],item.id);
 }
});
