const fs=require('node:fs'),fsp=fs.promises,path=require('node:path'),crypto=require('node:crypto'),zlib=require('node:zlib');
const library=require('./library');
const MAX_RAW=128*1024*1024,MAX_ZIP=64*1024*1024;
const canonical=p=>String(p||'').replace(/\\/g,'/').replace(/\/+$/,'').toLowerCase();
const digest=s=>crypto.createHash('sha256').update(s).digest('hex');
const unique=a=>[...new Set(a)];
const text=(v,max=4096)=>typeof v==='string'?v.slice(0,max):'';
function safeRelative(p){return typeof p==='string'&&!path.win32.isAbsolute(p)&&!path.posix.isAbsolute(p)&&!p.replace(/\\/g,'/').split('/').some(x=>x==='..')&&!p.includes('\0')&&!p.includes(':');}
function waveValid(w){return w&&Number.isFinite(w.duration)&&w.duration>=0&&Array.isArray(w.peaks)&&w.peaks.length===900&&w.peaks.every(v=>Number.isFinite(v)&&v>=0&&v<=1);}
function uid(e){return digest(JSON.stringify([canonical(e.originalPath),e.sha256||'',e.size]));}
function cleanUI(ui={}){ui=ui||{};const out={};for(const k of ['autoPreview','showNameTags','showFolders','previewMuted','loopSelection'])if(typeof ui[k]==='boolean')out[k]=ui[k];if(Number.isFinite(ui.previewVolume))out.previewVolume=Math.max(0,Math.min(100,ui.previewVolume));if(['relevance','name','recent'].includes(ui.sortMode))out.sortMode=ui.sortMode;if(['all','wav','mp3','flac','aiff','m4a','ogg'].includes(ui.extension))out.extension=ui.extension;return out;}
function validate(data){
 if(!data||data.format!=='hnh-sfx-backup'||data.version!==1||!Array.isArray(data.files)||data.files.length>100000)throw Error('File không phải bản sao lưu HNH được hỗ trợ (schema 1).');
 const files=data.files.map(raw=>{
  if(!raw||!text(raw.originalPath)||!text(raw.filename)||!safeRelative(raw.relativePath)||!Number.isFinite(raw.size)||raw.size<0)throw Error('Thông tin file trong bản sao lưu không hợp lệ.');
  const sha256=/^[a-f0-9]{64}$/.test(raw.sha256||'')?raw.sha256:null;
  const e={originalPath:text(raw.originalPath),root:text(raw.root),relativePath:text(raw.relativePath),filename:text(raw.filename,512),size:raw.size,mtimeMs:Number(raw.mtimeMs)||0,sha256,tags:unique((Array.isArray(raw.tags)?raw.tags:[]).filter(t=>typeof t==='string').map(t=>t.slice(0,160))).slice(0,100),favorite:raw.favorite===true,recent:Number.isFinite(raw.recent)?raw.recent:null,usage:Number.isFinite(raw.usage)?Math.max(0,raw.usage):0};
  e.addedAt=Number.isFinite(raw.addedAt)&&raw.addedAt>0?raw.addedAt:null;
  if(Number.isFinite(raw.duration)&&raw.duration>=0)e.duration=raw.duration;
  if(sha256&&waveValid(raw.waveform))e.waveform=raw.waveform;
  e.uid=uid(e);return e;
 });
 const ui=cleanUI(data.settings?.ui),collections=[];
 const libraries=(Array.isArray(data.settings?.libraries)?data.settings.libraries:[]).slice(0,1000).filter(l=>l&&typeof l.path==='string').map(l=>({path:text(l.path),types:require('./library-types').types(l),enabled:l.enabled!==false}));
 for(const c of (Array.isArray(data.settings?.collections)?data.settings.collections:[]).slice(0,500)){if(!c||typeof c.name!=='string'||!c.filters)continue;const f={};for(const k of ['query','extension','library','min','max','tag','mode','sort','addedMode','addedFrom','addedTo'])if(typeof c.filters[k]==='string')f[k]=c.filters[k].slice(0,4096);if(Array.isArray(c.filters.sidebarTags))f.sidebarTags=[...new Set(c.filters.sidebarTags.filter(t=>typeof t==='string').map(t=>t.slice(0,160)))].slice(0,100);if(['all','any'].includes(c.filters.tagMatch))f.tagMatch=c.filters.tagMatch;if(c.filters.folder&&typeof c.filters.folder.root==='string'&&safeRelative(c.filters.folder.path))f.folder={root:text(c.filters.folder.root),path:text(c.filters.folder.path)};collections.push({name:c.name.slice(0,80),filters:f});}
 return {format:'hnh-sfx-backup',version:1,createdAt:text(data.createdAt,80),files,settings:{ui,collections,libraries}};
}
async function readJSON(file,fallback){try{return JSON.parse(await fsp.readFile(file,'utf8'));}catch(e){if(e.code==='ENOENT')return fallback;throw e;}}
async function atomic(file,data){await fsp.mkdir(path.dirname(file),{recursive:true});const tmp=file+'.'+crypto.randomUUID()+'.tmp';try{await fsp.writeFile(tmp,JSON.stringify(data),'utf8');await fsp.rename(tmp,file);}finally{await fsp.unlink(tmp).catch(()=>{});}}
function mergeEntries(entries){const map=new Map();for(const e of entries){const id=uid(e),before=map.get(id);map.set(id,before?{...before,...e,tags:unique([...before.tags,...e.tags]),favorite:before.favorite||e.favorite,usage:Math.max(before.usage||0,e.usage||0),waveform:e.waveform||before.waveform}:e);}return [...map.values()];}
class Portable {
 constructor(directory,hooks){this.dir=directory;this.hooks=hooks;this.active=null;this.plan=null;this.identities=new Map();this.cancelled=false;this.busy=false;}
 progress(stage,current,total){this.hooks.progress?.({stage,current,total});if(this.cancelled)throw Error('Đã hủy. Dữ liệu hiện tại chưa thay đổi.');}
 async job(fn){if(this.busy)throw Error('Một thao tác sao lưu/khôi phục đang chạy.');this.busy=true;this.cancelled=false;try{return await fn();}finally{this.busy=false;this.hooks.progress?.({stage:'idle'});}}
 cancel(){this.cancelled=true;}
 async identity(file){
  const st=await fsp.stat(file.path);if(!st.isFile())throw Error('Không phải file âm thanh');const stamp=[st.size,st.mtimeMs,st.ctimeMs].join('|'),key=canonical(file.path),old=this.identities.get(key);if(old?.stamp===stamp)return old;
  const hash=crypto.createHash('sha256'),stream=fs.createReadStream(file.path);try{for await(const chunk of stream){if(this.cancelled){stream.destroy();throw Error('Đã hủy.');}hash.update(chunk);}}finally{stream.destroy();}
  const after=await fsp.stat(file.path);if(after.size!==st.size||after.mtimeMs!==st.mtimeMs||after.ctimeMs!==st.ctimeMs)throw Error('File thay đổi trong lúc đọc: '+file.path);
  const result={hash:hash.digest('hex'),stamp,size:st.size,mtimeMs:st.mtimeMs,ctimeMs:st.ctimeMs};this.identities.set(key,result);return result;
 }
 async exportFile(destination,options={}){return this.job(async()=>{
  const settings=await this.hooks.loadSettings(),index=await this.hooks.loadIndex();let memo=await readJSON(path.join(this.dir,'identity-cache.json'),{});for(const [k,v] of Object.entries(memo))if(v&&/^[a-f0-9]{64}$/.test(v.hash))this.identities.set(k,v);
  const favorites=new Set(settings.favorites||[]),recent=new Map((settings.recent||[]).map((p,i)=>[p,i])),source=new Map(index.files.map(f=>[f.path,f]));
  for(const p of unique([...favorites,...Object.keys(settings.tags||{})]))if(!source.has(p)){const root=(settings.libraries||[]).map(l=>l.path).filter(r=>canonical(p).startsWith(canonical(r)+'/')).sort((a,b)=>b.length-a.length)[0]||'';source.set(p,{path:p,root,relativePath:root?path.relative(root,p):path.basename(p),filename:path.basename(p),size:0,mtimeMs:0});}
  const entries=[],warnings=[];let i=0;
  for(const f of source.values()){
   this.progress('Đọc nhận diện nội dung',++i,source.size);const e={originalPath:f.path,root:f.root||'',relativePath:f.relativePath||f.filename,filename:f.filename,size:f.size||0,mtimeMs:f.mtimeMs||0,addedAt:f.addedAt??index.addedHistory?.[path.resolve(f.path).toLowerCase()]??null,duration:f.duration,tags:settings.tags?.[f.path]||[],favorite:favorites.has(f.path),recent:options.history?recent.get(f.path):null,usage:options.history?settings.usage?.[f.path]||0:0,sha256:null};
   try{const identity=await this.identity(f);e.sha256=identity.hash;e.size=identity.size;e.mtimeMs=identity.mtimeMs;if(identity.size!==f.size||identity.mtimeMs!==f.mtimeMs)delete e.duration;
    if(options.waveforms&&identity.size===f.size&&identity.mtimeMs===f.mtimeMs){const w=await readJSON(path.join(this.dir,'waveforms-v1',library.fingerprint(f)+'.json'),null);if(waveValid(w))e.waveform=w;}
   }catch(error){if(this.cancelled)throw error;warnings.push(f.path+': '+error.message);}
   entries.push(e);
  }
  const pending=await readJSON(path.join(this.dir,'pending-restores.json'),null);if(pending)entries.push(...validate(pending).files);
  for(const e of entries){if(!options.waveforms)delete e.waveform;if(!options.history){e.recent=null;e.usage=0;}}
  const data=validate({format:'hnh-sfx-backup',version:1,createdAt:new Date().toISOString(),files:mergeEntries(entries),settings:{libraries:settings.libraries,ui:settings.ui,collections:[...(settings.collections||[]),...(pending?.settings?.collections||[])]}});
  this.progress('Đóng gói bản sao lưu',1,1);const raw=Buffer.from(JSON.stringify(data));if(raw.length>MAX_RAW)throw Error('Bản lưu quá lớn; thử bỏ tùy chọn waveform cache.');const bytes=await new Promise((resolve,reject)=>zlib.gzip(raw,(e,b)=>e?reject(e):resolve(b)));if(bytes.length>MAX_ZIP)throw Error('Gói lưu vượt 64 MB; thử bỏ waveform cache.');
  this.progress('Ghi bản sao lưu',1,1);const tmp=destination+'.'+crypto.randomUUID()+'.tmp';try{await fsp.writeFile(tmp,bytes);await fsp.rename(tmp,destination);}finally{await fsp.unlink(tmp).catch(()=>{});}
  await atomic(path.join(this.dir,'identity-cache.json'),Object.fromEntries(this.identities)).catch(e=>warnings.push('Không lưu được cache nhận diện: '+e.message));
  return {path:destination,files:data.files.length,waveforms:data.files.filter(e=>e.waveform).length,warnings:warnings.slice(0,20),warningCount:warnings.length};
 });}
 async openLibrary(root){return this.job(async()=>{
  this.active=null;this.plan=null;this.relinkRoot=null;const settings=await this.hooks.loadSettings(),index=await this.hooks.loadIndex();const lib=(settings.libraries||[]).find(l=>canonical(l.path)===canonical(root));if(!lib)throw Error('Thư viện không còn trong cấu hình.');const inside=p=>canonical(p).startsWith(canonical(lib.path)+'/');const sources=new Map(index.files.filter(f=>inside(f.path)).map(f=>[f.path,f]));for(const p of unique([...Object.keys(settings.tags||{}),...(settings.favorites||[]),...(settings.recent||[])]))if(inside(p)&&!sources.has(p))sources.set(p,{path:p,filename:path.basename(p),relativePath:path.relative(lib.path,p),size:0,mtimeMs:0});if(!sources.size)throw Error('Không còn index hoặc metadata cho thư viện này. Hãy mở bản sao lưu cũ nếu có.');const memo=await readJSON(path.join(this.dir,'identity-cache.json'),{}),entries=[];let n=0;
  for(const f of sources.values()){this.progress('Đọc index và metadata đã lưu',++n,sources.size);const id=memo[canonical(f.path)],known=id&&id.size===f.size&&id.mtimeMs===f.mtimeMs&&/^[a-f0-9]{64}$/.test(id.hash);const e={originalPath:f.path,root:lib.path,relativePath:path.relative(lib.path,f.path),filename:f.filename,size:f.size||0,mtimeMs:f.mtimeMs||0,addedAt:f.addedAt,duration:f.duration,sha256:known?id.hash:null,tags:settings.tags?.[f.path]||[],favorite:(settings.favorites||[]).includes(f.path),recent:(settings.recent||[]).includes(f.path)?settings.recent.indexOf(f.path):null,usage:settings.usage?.[f.path]||0};if(known){const wave=await readJSON(path.join(this.dir,'waveforms-v1',library.fingerprint(f)+'.json'),null);if(waveValid(wave))e.waveform=wave;}entries.push(e);}
  this.active=validate({format:'hnh-sfx-backup',version:1,createdAt:new Date().toISOString(),files:entries,settings:{libraries:[lib],collections:settings.collections,ui:settings.ui}});this.relinkRoot=lib.path;return {...this.summary(),relink:true};
 });}
 async openFile(file){this.relinkRoot=null;const st=await fsp.stat(file);if(st.size>MAX_ZIP)throw Error('File sao lưu vượt giới hạn 64 MB.');const bytes=await fsp.readFile(file);let raw;try{raw=zlib.gunzipSync(bytes,{maxOutputLength:MAX_RAW});}catch{throw Error('Không đọc được gói sao lưu hoặc gói vượt giới hạn giải nén.');}this.active=validate(JSON.parse(raw.toString('utf8')));this.plan=null;return this.summary();}
 async openPending(){this.relinkRoot=null;const data=await readJSON(path.join(this.dir,'pending-restores.json'),null);if(!data||!data.files?.length)throw Error('Không có mục chờ nối lại.');this.active=validate(data);this.plan=null;return this.summary();}
 summary(){return {files:this.active.files.length,waveforms:this.active.files.filter(e=>e.waveform).length,roots:unique([...this.active.files.map(e=>e.root),...this.active.settings.libraries.map(l=>l.path)].filter(Boolean)),createdAt:this.active.createdAt};}
 async prepare(mappings){return this.job(async()=>{
  if(!this.active)throw Error('Chưa mở bản sao lưu.');this.plan=null;const roots=new Set(this.summary().roots),map=new Map();
  for(const [oldRoot,newRoot] of Object.entries(mappings||{})){if(!roots.has(oldRoot)||!newRoot)continue;if(!path.isAbsolute(newRoot))throw Error('Cần chọn đường dẫn thư mục đầy đủ.');map.set(oldRoot,path.resolve(newRoot));}
  const settings=await this.hooks.loadSettings(),old=await this.hooks.loadIndex(),libs=[...(settings.libraries||[])];for(const [oldRoot,p] of map)if(!libs.some(l=>canonical(l.path)===canonical(p))){const saved=this.active.settings.libraries.find(l=>canonical(l.path)===canonical(oldRoot));libs.push({path:p,enabled:true,...(saved?{types:saved.types}:{})});}
  this.progress('Đối chiếu các thư mục đích',0,1);const index=await library.scan(libs,old),sizes=new Set(this.active.files.filter(e=>e.sha256).map(e=>e.size)),byHash=new Map(),byName=new Map(),byPath=new Map(),identities=new Map();let n=0;const warnings=[...index.warnings];
  const destination=this.relinkRoot&&map.get(this.relinkRoot);const candidatesIndex=destination?index.files.filter(f=>canonical(f.path).startsWith(canonical(destination)+'/')):index.files;
  for(const f of candidatesIndex){this.progress('Kiểm tra file trên máy này',++n,index.files.length);const name=f.filename.toLowerCase();if(!byName.has(name))byName.set(name,[]);byName.get(name).push(f);byPath.set(canonical(f.path),f);if(!sizes.has(f.size))continue;
   try{const id=await this.identity(f);identities.set(canonical(f.path),id);if(!byHash.has(id.hash))byHash.set(id.hash,[]);byHash.get(id.hash).push(f);}catch(e){if(this.cancelled)throw e;warnings.push(f.path+': '+e.message);}
  }
  const rows=this.active.files.map((e,i)=>{
   let expected=null;if(map.has(e.root))expected=canonical(path.join(map.get(e.root),e.relativePath));else expected=canonical(e.originalPath);
   const exact=e.sha256?(byHash.get(e.sha256)||[]):[],preferred=exact.find(f=>canonical(f.path)===expected);
   const target=preferred||(exact.length===1?exact[0]:null);let candidates=exact;
   if(!candidates.length)candidates=(byName.get(e.filename.toLowerCase())||[]).filter(f=>!e.size||f.size===e.size);
   const relative=byPath.get(expected);if(!target&&relative&&!candidates.some(f=>f.path===relative.path))candidates=[relative,...candidates];
   const suggested=!target&&candidates.length===1&&e.size>0&&candidates[0].size===e.size&&canonical(candidates[0].path)===expected?candidates[0].path:null;return {id:i,entry:e,suggested,status:target?'exact':candidates.length?'review':'missing',target,candidates:candidates.slice(0,20)};
  });
  this.plan={token:crypto.randomUUID(),rows,index,libs,map,identities,manifest:this.active};
  return {token:this.plan.token,exact:rows.filter(r=>r.status==='exact').length,review:rows.filter(r=>r.status==='review').length,missing:rows.filter(r=>r.status==='missing').length,total:rows.length,warnings:warnings.slice(0,20),rows:rows.filter(r=>r.status!=='exact').map(r=>({id:r.id,name:r.entry.filename,originalPath:r.entry.originalPath,tags:r.entry.tags,favorite:r.entry.favorite,status:r.status,suggested:r.suggested,candidates:r.candidates.map(f=>({path:f.path,duration:f.duration,size:f.size}))}))};
 });}
 async snapshot(){const dir=path.join(this.dir,'restore-backups',Date.now()+'-'+crypto.randomUUID());await fsp.mkdir(dir,{recursive:true});const names=['settings.json','sfx-index.json','pending-restores.json'];const exists={};for(const name of names){try{await fsp.copyFile(path.join(this.dir,name),path.join(dir,name));exists[name]=true;}catch(e){if(e.code!=='ENOENT')throw e;exists[name]=false;}}await atomic(path.join(dir,'snapshot.json'),{exists});return dir;}
 async restoreSnapshot(dir){
  const parent=path.resolve(this.dir,'restore-backups')+path.sep;if(!path.resolve(dir).startsWith(parent))throw Error('Đường dẫn backup không hợp lệ.');const snap=await readJSON(path.join(dir,'snapshot.json'),null);if(!snap)throw Error('Không có bản sao trước nhập.');
  for(const name of ['settings.json','sfx-index.json','pending-restores.json']){if(snap.exists[name]){const data=await readJSON(path.join(dir,name),null);await atomic(path.join(this.dir,name),data);}else await fsp.unlink(path.join(this.dir,name)).catch(e=>{if(e.code!=='ENOENT')throw e;});}
 }
 async recover(){const journal=path.join(this.dir,'restore-journal.json'),data=await readJSON(journal,null);if(data){await this.restoreSnapshot(data.backup);await fsp.unlink(journal);}}
 async apply(token,overrides={},options={}){return this.job(async()=>{
  const p=this.plan;if(!p||token!==p.token)throw Error('Kết quả đã hết hiệu lực. Hãy đối chiếu lại.');
  const settings=await this.hooks.loadSettings(),index=await this.hooks.loadIndex(),tags=new Map(Object.entries(settings.tags||{})),favorites=new Set(settings.favorites||[]),usage={...settings.usage},recent=[...(settings.recent||[])],paths=new Map(),accepted=[],remaining=[];
  for(const row of p.rows){this.progress('Chuẩn bị khôi phục',row.id+1,p.rows.length);let target=row.target;const chosen=overrides[row.id];if(chosen){target=row.candidates.find(f=>f.path===chosen);if(!target)throw Error('File xác nhận không có trong danh sách đối chiếu.');}
   if(!target){remaining.push(row.entry);continue;}const st=await fsp.stat(target.path).catch(()=>null),identity=p.identities.get(canonical(target.path));if(!st||st.size!==target.size||st.mtimeMs!==target.mtimeMs||(identity&&st.ctimeMs!==identity.ctimeMs))throw Error('File đích đã thay đổi. Hãy đối chiếu lại: '+target.path);
   const e=row.entry,verified=!!e.sha256&&identity?.hash===e.sha256;tags.set(target.path,unique([...(tags.get(target.path)||[]),...e.tags]));if(e.favorite)favorites.add(target.path);if(options.history){usage[target.path]=Math.max(usage[target.path]||0,e.usage||0);if(e.recent!==null&&!recent.includes(target.path))recent.push(target.path);}paths.set(e.originalPath,target.path);accepted.push({entry:e,target,verified});
  }
  const originalFiles=new Map(index.files.map(f=>[canonical(f.path),f]));
  const mergedFiles=new Map(index.files.map(f=>[canonical(f.path),f]));for(const f of p.index.files)mergedFiles.set(canonical(f.path),f);
  const addedHistory={...(p.index.addedHistory||{}),...(index.addedHistory||{})};
  for(const a of accepted){const key=path.resolve(a.target.path).toLowerCase(),existing=originalFiles.get(canonical(a.target.path));const date=existing?existing.addedAt:(Object.prototype.hasOwnProperty.call(index.addedHistory||{},key)?index.addedHistory[key]:a.entry.addedAt);addedHistory[key]=Number.isFinite(date)&&date>0?date:null;const f=mergedFiles.get(canonical(a.target.path));if(f)f.addedAt=addedHistory[key];}
  for(const a of accepted)if(a.verified&&Number.isFinite(a.entry.duration)){const f=mergedFiles.get(canonical(a.target.path));if(f)f.duration=a.entry.duration;}
  const libraries=[...(settings.libraries||[])];for(const l of p.libs)if(!libraries.some(x=>canonical(x.path)===canonical(l.path)))libraries.push(l);
  const collections=[...(settings.collections||[])];if(options.collections!==false)for(const c of p.manifest.settings.collections){const f={...c.filters};if(f.library&&f.library!=='all'){const mapped=p.map.get(f.library);if(mapped)f.library=mapped;else if(!libraries.some(l=>canonical(l.path)===canonical(f.library)))continue;}if(f.folder){const mapped=p.map.get(f.folder.root);if(mapped)f.folder={...f.folder,root:mapped};else if(!libraries.some(l=>canonical(l.path)===canonical(f.folder.root)))continue;}if(!collections.some(x=>x.name===c.name&&JSON.stringify(x.filters)===JSON.stringify(f)))collections.push({id:crypto.randomUUID(),name:c.name,filters:f});}
  const next={...settings,libraries,tags:Object.fromEntries(tags),favorites:[...favorites],collections};if(options.history){next.recent=recent.slice(0,100);next.usage=usage;}if(options.ui)next.ui={...settings.ui,...p.manifest.settings.ui};
  const pendingOld=await readJSON(path.join(this.dir,'pending-restores.json'),null),processed=new Set(p.rows.map(r=>uid(r.entry)));const pending=validate({format:'hnh-sfx-backup',version:1,files:mergeEntries([...(pendingOld?validate(pendingOld).files.filter(e=>!processed.has(uid(e))):[]),...remaining]),settings:{libraries:p.manifest.settings.libraries,ui:p.manifest.settings.ui,collections:[...(pendingOld?.settings?.collections||[]),...p.manifest.settings.collections]}});
  this.progress('Lưu bản sao trước khi nhập',1,1);const backup=await this.snapshot(),journal=path.join(this.dir,'restore-journal.json');await atomic(journal,{backup});
  let cacheCount=0;try{
   // Commit core files as one recoverable operation; no cancellation after this point.
   await atomic(path.join(this.dir,'settings.json'),next);await atomic(path.join(this.dir,'sfx-index.json'),{...index,version:3,addedHistory,files:[...mergedFiles.values()]});await atomic(path.join(this.dir,'pending-restores.json'),pending);
   await atomic(path.join(this.dir,'last-restore.json'),{backup});await fsp.unlink(journal);
  }catch(e){await this.restoreSnapshot(backup);await fsp.unlink(journal).catch(()=>{});throw e;}
  if(options.waveforms!==false)for(const {entry,target,verified} of accepted){if(verified&&entry.waveform&&cacheCount<2000){try{await atomic(path.join(this.dir,'waveforms-v1',library.fingerprint(target)+'.json'),entry.waveform);cacheCount++;}catch{/* cache is optional; core restore already committed */}}}
  this.plan=null;return {matched:accepted.length,pending:pending.files.length,waveforms:cacheCount,backup,settings:next,index:await this.hooks.loadIndex()};
 });}
 async undo(){return this.job(async()=>{const last=await readJSON(path.join(this.dir,'last-restore.json'),null);if(!last)throw Error('Chưa có lần nhập nào để hoàn tác.');const safety=await this.snapshot(),journal=path.join(this.dir,'restore-journal.json');await atomic(journal,{backup:safety});try{await this.restoreSnapshot(last.backup);await fsp.unlink(journal);await fsp.unlink(path.join(this.dir,'last-restore.json'));}catch(e){await this.restoreSnapshot(safety);await fsp.unlink(journal).catch(()=>{});throw e;}this.plan=null;return {settings:await this.hooks.loadSettings(),index:await this.hooks.loadIndex(),backup:safety};});}
}
module.exports={Portable,validate,waveValid,canonical};
