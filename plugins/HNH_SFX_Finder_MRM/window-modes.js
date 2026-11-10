const fs=require('node:fs'),path=require('node:path');
const shortcuts=require('./UI/shortcuts');

const NORMAL_MIN={width:760,height:680},DOCK_MIN={width:340,height:280};
const SIDE=new Set(['left','right']),EDGES=new Set(['left','right','top','bottom']);
const DEFAULT_SIZE={side:460,band:340};
const isSide=edge=>SIDE.has(edge);

function clampSize(edge,value,workArea){
 const side=isSide(edge),min=side?DOCK_MIN.width:DOCK_MIN.height,max=Math.max(min,Math.floor((side?workArea.width:workArea.height)*.7));
 const n=Number(value);
 return Math.min(max,Math.max(min,Math.round(Number.isFinite(n)?n:DEFAULT_SIZE[side?'side':'band'])));
}
function dockBounds(workArea,edge,size){
 const s=clampSize(edge,size,workArea),{x,y,width,height}=workArea;
 if(edge==='left')return {x,y,width:s,height};
 if(edge==='right')return {x:x+width-s,y,width:s,height};
 if(edge==='top')return {x,y,width,height:s};
 return {x,y:y+height-s,width,height:s};
}
function validBounds(b){return !!b&&['x','y','width','height'].every(k=>Number.isFinite(b[k]))&&b.width>0&&b.height>0;}
function normalizePrefs(raw){
 const r=raw&&typeof raw==='object'?raw:{},size=r.dockSize&&typeof r.dockSize==='object'?r.dockSize:{};
 return {
  pinned:r.pinned===true,
  dock:EDGES.has(r.dock)?r.dock:null,
  dockSize:{side:Number(size.side)||DEFAULT_SIZE.side,band:Number(size.band)||DEFAULT_SIZE.band},
  normalBounds:validBounds(r.normalBounds)?r.normalBounds:null,
  shortcuts:shortcuts.normalize(r.shortcuts)
 };
}

function createWindowModes({screen,globalShortcut,file,send,platform=process.platform}){
 let win=null,snapping=false,failed=[];
 let prefs=normalizePrefs((()=>{try{return JSON.parse(fs.readFileSync(file,'utf8'));}catch{return null;}})());
 const alive=()=>win&&!win.isDestroyed();
 function save(){try{fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,JSON.stringify(prefs,null,1));}catch{}}
 const state=()=>({pinned:prefs.pinned,dock:prefs.dock,onTop:prefs.pinned||!!prefs.dock,shortcuts:prefs.shortcuts,failedShortcuts:failed});
 const publish=()=>{send('window-modes:changed',state());return state();};
 function applyTop(){
  if(!alive())return;
  const on=prefs.pinned||!!prefs.dock;
  // Windows: mức 'floating' bị Electron xếp sau taskbar và có thể mất cờ topmost.
  win.setAlwaysOnTop(on,platform==='win32'?'pop-up-menu':'floating');
  if(platform==='darwin')win.setVisibleOnAllWorkspaces(on,{visibleOnFullScreen:on,skipTransformProcessType:true});
 }
 const sizeKey=()=>isSide(prefs.dock)?'side':'band';
 function snap(){
  if(!alive()||!prefs.dock)return;
  const display=screen.getDisplayMatching(win.getBounds());
  snapping=true;
  if(win.isMaximized())win.unmaximize();
  if(win.isFullScreen())win.setFullScreen(false);
  win.setMinimumSize(DOCK_MIN.width,DOCK_MIN.height);
  win.setBounds(dockBounds(display.workArea,prefs.dock,prefs.dockSize[sizeKey()]));
  setTimeout(()=>{snapping=false;},150);
 }
 function onResized(){
  if(snapping||!alive()||!prefs.dock)return;
  const b=win.getBounds();
  prefs.dockSize[sizeKey()]=isSide(prefs.dock)?b.width:b.height;
  save();snap();
 }
 function onMoved(){if(!snapping&&alive()&&prefs.dock)snap();}
 const onDisplay=()=>snap();
 function applyGlobal(){
  failed=[];
  globalShortcut.unregisterAll();
  if(!alive()||!prefs.shortcuts.globalEnabled)return;
  for(const [action,combo] of Object.entries(prefs.shortcuts.global)){
   let ok=false;
   try{ok=globalShortcut.register(combo,()=>send('shortcut:global',action));}catch{}
   if(!ok)failed.push({action,combo});
  }
 }
 function setDock(edge){
  const next=EDGES.has(edge)?edge:null;
  if(!alive()||next===prefs.dock)return state();
  if(next&&!prefs.dock)prefs.normalBounds=win.isMaximized()?win.getNormalBounds():win.getBounds();
  prefs.dock=next;
  if(next)snap();
  else{
   win.setMinimumSize(NORMAL_MIN.width,NORMAL_MIN.height);
   if(prefs.normalBounds)win.setBounds(prefs.normalBounds);
  }
  applyTop();save();
  return publish();
 }
 function setPinned(on){prefs.pinned=on===true;applyTop();save();return publish();}
 function setShortcuts(raw){prefs.shortcuts=shortcuts.normalize(raw);applyGlobal();save();return publish();}
 // Gọi trước khi hiện cửa sổ: dùng các giá trị đã lưu.
 function attach(window){
  win=window;
  if(prefs.dock)snap();
  applyTop();applyGlobal();
  win.on('resized',onResized);win.on('moved',onMoved);
  screen.on('display-metrics-changed',onDisplay);screen.on('display-removed',onDisplay);
  win.once('closed',()=>{win=null;globalShortcut.unregisterAll();screen.removeListener('display-metrics-changed',onDisplay);screen.removeListener('display-removed',onDisplay);});
 }
 return {attach,setDock,setPinned,setShortcuts,state};
}
module.exports={createWindowModes,dockBounds,clampSize,normalizePrefs,NORMAL_MIN,DOCK_MIN};
