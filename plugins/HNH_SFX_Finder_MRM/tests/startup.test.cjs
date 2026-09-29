const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=path.resolve(__dirname,'..');
const temp=fs.mkdtempSync(path.join(require('os').tmpdir(),'hnh-startup-test-'));
const defaults={libraries:[],favorites:[],recent:[],usage:{},ui:{}};
// Plugin lai: máy không có MRM (MRM_DB trỏ tới file không tồn tại) -> chạy độc lập như bản gốc,
// dữ liệu HNH SFX Finder 0.10.2 được sao chép sang thư mục standalone lần đầu.
process.env.MRM_DB=path.join(temp,'khong-co-mrm','nink-vault.db');
test('startup returns cached files before a slow scan; bridge retries independently; no manual reconnect',async()=>{
 const data=path.join(temp,'app'),dir=path.join(data,'HNH-SFX-Finder');fs.mkdirSync(dir,{recursive:true});
 const settings={...defaults,libraries:[{path:'D:\\SFX'}],favorites:['air.wav'],tags:{'air.wav':['tag']}};fs.writeFileSync(path.join(dir,'settings.json'),JSON.stringify(settings));fs.writeFileSync(path.join(dir,'sfx-index.json'),JSON.stringify({version:3,files:[{filename:'air.wav',path:'D:\\SFX\\air.wav',root:'D:\\SFX'}]}));
 const handlers={},events=[],lifecycle={},timers=new Set();let releaseScan,scanStarted=false,attempts=0;
 const scanPromise=new Promise(r=>releaseScan=r);
 class Watcher{start(){}close(){}}
 const lib={...require(path.join(root,'library')),Watcher,scan:async()=>{scanStarted=true;return scanPromise;}};
 const mod={GetInfo:()=>({version:'2.0.0'}),Initialize:()=>++attempts>=3,GetResolve:()=>({GetProductName:()=> 'Resolve',GetVersionString:()=> '20.2'}),CleanUp:()=>{}};
 const fakeRequire=n=>n==='electron'?{app:{getPath:()=>data,whenReady:()=>({then:()=>{}}),on:(k,fn)=>lifecycle[k]=fn,quit:()=>{}},ipcMain:{on:()=>{},removeListener:()=>{},handle:(k,fn)=>handlers[k]=fn}}:n.endsWith('.node')?mod:n==='./library'?lib:n.startsWith('./')?require(path.join(root,n)):require(n);
 fakeRequire.cache={};fakeRequire.resolve=n=>n;
 const ctx={require:fakeRequire,__dirname:root,process,console,setTimeout:(fn,ms)=>{const timer=setTimeout(fn,Math.min(ms,20));timers.add(timer);return timer;},clearTimeout,setInterval,clearInterval};vm.createContext(ctx);vm.runInContext(fs.readFileSync(root+'/main.js','utf8')+'\nregisterIpc();',ctx);ctx.__events=events;vm.runInContext("mainWindow={isDestroyed:()=>false,webContents:{send:(channel,data)=>__events.push({channel,data})}}",ctx);
 try{const result=await handlers['app:get-state']();assert.equal(result.index.files.length,1);assert.equal(result.settings.favorites[0],'air.wav');assert.equal(scanStarted,false);assert.equal(result.backgroundScan,true);assert.equal(result.mrm.mode,'standalone');assert(!fs.existsSync(process.env.MRM_DB),'không tạo DB MRM');await new Promise(r=>setTimeout(r,160));assert(scanStarted);assert.equal(attempts,3);assert(events.some(e=>e.channel==='resolve:changed'&&e.data.ok));assert(!events.some(e=>e.channel==='library:changed'));releaseScan({version:3,files:result.index.files,stats:{added:0,changed:0,removed:0,unchanged:1},warnings:[]});for(let wait=0;wait<100&&!events.some(e=>e.channel==='library:changed');wait++)await new Promise(r=>setTimeout(r,20));assert(events.some(e=>e.channel==='library:changed'&&e.data.index.files.length===1));assert.equal((await handlers['app:get-state']()).backgroundScan,false);assert.equal(JSON.parse(fs.readFileSync(path.join(dir,'settings.json'),'utf8')).favorites[0],'air.wav');}finally{lifecycle['window-all-closed']();for(const t of timers)clearTimeout(t);}
});
