const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),{EventEmitter}=require('node:events');
const {createWindowModes,dockBounds,normalizePrefs}=require('../window-modes');

const area={x:1920,y:40,width:2560,height:1400};
test('dock bounds stay flush with each edge of the work area, including a second monitor',()=>{
 assert.deepEqual(dockBounds(area,'left',460),{x:1920,y:40,width:460,height:1400});
 assert.deepEqual(dockBounds(area,'right',460),{x:4020,y:40,width:460,height:1400});
 assert.deepEqual(dockBounds(area,'top',340),{x:1920,y:40,width:2560,height:340});
 assert.deepEqual(dockBounds(area,'bottom',340),{x:1920,y:1100,width:2560,height:340});
});
test('dock size is clamped so the panel never swallows the screen',()=>{
 assert.equal(dockBounds(area,'left',10).width,340);
 assert.equal(dockBounds(area,'left',99999).width,1792);
 assert.equal(dockBounds(area,'top',10).height,280);
});
test('damaged preferences fall back to safe defaults',()=>{
 const p=normalizePrefs({pinned:'yes',dock:'middle',normalBounds:{x:1},shortcuts:'x'});
 assert.equal(p.pinned,false);assert.equal(p.dock,null);assert.equal(p.normalBounds,null);assert.equal(p.shortcuts.globalEnabled,false);
});

function fakes(){
 class Win extends EventEmitter{bounds={x:100,y:100,width:1040,height:820};min=[760,680];top=null;maximized=false;
  isDestroyed(){return false;}isMaximized(){return this.maximized;}isFullScreen(){return false;}unmaximize(){this.maximized=false;}setFullScreen(){}
  getBounds(){return {...this.bounds};}getNormalBounds(){return {...this.bounds};}setBounds(b){this.bounds={...b};}setMinimumSize(w,h){this.min=[w,h];}
  setAlwaysOnTop(on,level){this.top={on,level};}setVisibleOnAllWorkspaces(){}}
 const screen=new EventEmitter();screen.getDisplayMatching=()=>({workArea:{x:0,y:0,width:1920,height:1040}});
 const registered=new Map(),globalShortcut={register:(c,fn)=>{if(c==='Ctrl+Alt+P')return false;registered.set(c,fn);return true;},unregisterAll:()=>registered.clear()};
 const file=path.join(fs.mkdtempSync(path.join(os.tmpdir(),'hnh-window-')),'prefs.json'),sent=[];
 const make=()=>createWindowModes({screen,globalShortcut,file,send:(c,d)=>sent.push([c,d]),platform:'win32'});
 return {win:new Win(),Win,screen,registered,file,sent,make};
}
test('docking snaps to the edge, floats, and undocking restores the window and the pin choice',()=>{
 const f=fakes(),m=f.make();m.attach(f.win);
 assert.equal(m.setDock('right').onTop,true);
 assert.deepEqual(f.win.bounds,{x:1460,y:0,width:460,height:1040});
 assert.deepEqual(f.win.top,{on:true,level:'floating'});assert.deepEqual(f.win.min,[340,280]);
 m.setDock(null);
 assert.deepEqual(f.win.bounds,{x:100,y:100,width:1040,height:820});
 assert.equal(f.win.top.on,false);assert.deepEqual(f.win.min,[760,680]);
 m.setPinned(true);m.setDock('left');m.setDock(null);
 assert.equal(f.win.top.on,true);
});
test('pin and dock survive a restart',()=>{
 const f=fakes(),m=f.make();m.attach(f.win);m.setPinned(true);m.setDock('bottom');
 const next=new f.Win(),again=f.make();again.attach(next);
 assert.equal(again.state().pinned,true);assert.equal(again.state().dock,'bottom');
 assert.deepEqual(next.bounds,{x:0,y:700,width:1920,height:340});assert.equal(next.top.on,true);
});
test('a resized dock keeps its new size and re-snaps',async()=>{
 const f=fakes(),m=f.make();m.attach(f.win);m.setDock('right');
 await new Promise(r=>setTimeout(r,170));
 f.win.bounds={x:1320,y:0,width:600,height:1040};f.win.emit('resized');
 assert.deepEqual(f.win.bounds,{x:1320,y:0,width:600,height:1040});
 await new Promise(r=>setTimeout(r,170));
 f.win.bounds={x:300,y:200,width:600,height:500};f.win.emit('moved');
 assert.deepEqual(f.win.bounds,{x:1320,y:0,width:600,height:1040});
});
test('system-wide shortcuts stay off until enabled, and report keys another app holds',()=>{
 const f=fakes(),m=f.make();m.attach(f.win);
 assert.equal(f.registered.size,0);
 const s=m.setShortcuts({...m.state().shortcuts,globalEnabled:true});
 assert.ok(f.registered.has('Ctrl+Alt+Space'));assert.deepEqual(s.failedShortcuts,[{action:'insertPlayhead',combo:'Ctrl+Alt+P'}]);
 f.registered.get('Ctrl+Alt+I')();
 assert.deepEqual(f.sent.at(-1).slice(0,2),['shortcut:global','insertRange']);
 m.setShortcuts({...m.state().shortcuts,globalEnabled:false});
 assert.equal(f.registered.size,0);
});
test('closing the window releases system-wide shortcuts',()=>{
 const f=fakes(),m=f.make();m.attach(f.win);m.setShortcuts({...m.state().shortcuts,globalEnabled:true});
 assert.ok(f.registered.size>0);f.win.emit('closed');assert.equal(f.registered.size,0);
});
