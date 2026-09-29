const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const fsp = fs.promises;
const {extensions,kind}=require('./media-types');
const {types}=require('./library-types');
const norm = s => String(s || '').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/[đĐ]/g,'d').toLowerCase().replace(/[_\-.()[\]{}]+/g,' ').replace(/\s+/g,' ').trim();
const key = p => process.platform === 'win32' ? path.resolve(p).toLowerCase() : path.resolve(p);
const fingerprint = f => crypto.createHash('sha256').update(`${key(f.path)}|${f.size}|${f.mtimeMs}`).digest('hex');
async function scan(libraries, previous = { files: [] }) {
  const old = new Map(previous.files.map(f => [key(f.path), f]));
  const addedHistory={...(previous.addedHistory||{})},now=Date.now();
  for(const f of previous.files)if(!Object.prototype.hasOwnProperty.call(addedHistory,key(f.path)))addedHistory[key(f.path)]=Number.isFinite(f.addedAt)&&f.addedAt>0?f.addedAt:null;
  const found = new Map(), warnings = [], stats = { added:0, changed:0, removed:0, unchanged:0 };
  for (const lib of libraries.filter(l => l.enabled !== false)) {
    const allowed=new Set(types(lib));
    const stack = [lib.path];
    while (stack.length) {
      const dir = stack.pop(); let entries;
      try { entries = await fsp.readdir(dir, {withFileTypes:true}); }
      catch (e) {
        warnings.push(`${dir}: ${e.code}`);
        // Offline/inaccessible directories must not erase their last known records.
        for (const f of previous.files) if (key(f.path).startsWith(key(dir) + path.sep) && !path.basename(f.path).startsWith('._') && allowed.has(kind(f.extension||path.extname(f.path).slice(1)))) found.set(key(f.path), f);
        continue;
      }
      for (const ent of entries) {
        if (ent.name.startsWith('._') || ent.name === '__MACOSX') continue;
        const full = path.join(dir, ent.name);
        if (ent.isDirectory()) { stack.push(full); continue; }
        const extension = path.extname(ent.name).slice(1).toLowerCase();
        if (!ent.isFile() || !extensions.has(extension) || !allowed.has(kind(extension)) || found.has(key(full))) continue;
        let st; try { st = await fsp.stat(full); } catch { continue; }
        const before = old.get(key(full));
        if(!Object.prototype.hasOwnProperty.call(addedHistory,key(full)))addedHistory[key(full)]=now;
        const addedAt=addedHistory[key(full)];
        if (before && before.size === st.size && before.mtimeMs === st.mtimeMs) {
          found.set(key(full), {...before, addedAt, root:lib.path, relativePath:path.relative(lib.path,full), folders:path.dirname(path.relative(lib.path,full)).split(path.sep)}); stats.unchanged++; continue;
        }
        const relativePath = path.relative(lib.path, full), folders = path.dirname(relativePath).split(path.sep);
        const basename = path.basename(full, path.extname(full));
        found.set(key(full), {id:key(full),addedAt,path:full,root:lib.path,relativePath,filename:ent.name,basename,extension,folders,size:st.size,mtimeMs:st.mtimeMs,searchText:norm([basename,...folders].join(' '))});
        stats[before ? 'changed' : 'added']++;
      }
    }
  }
  stats.removed = [...old.keys()].filter(k => !found.has(k)).length;
  return {version:3,addedHistory,generatedAt:new Date().toISOString(),files:[...found.values()],stats,warnings};
}
class Watcher {
  constructor(onChange) { this.onChange=onChange; this.handles=[]; this.timer=null; this.poll=null; }
  start(libraries) {
    this.close();
    for (const lib of libraries.filter(l=>l.enabled!==false)) {
      try { const w=fs.watch(lib.path,{recursive:true},(_event,name)=>{if(name && path.basename(String(name)).startsWith('._'))return; this.schedule();}); w.on('error',()=>this.schedule()); this.handles.push(w); } catch { /* periodic reconciliation retries unavailable roots */ }
    }
    this.poll=setInterval(()=>this.schedule(),60000); this.poll.unref?.();
  }
  schedule() { clearTimeout(this.timer); this.timer=setTimeout(()=>Promise.resolve(this.onChange()).catch(()=>{}),700); }
  close() { this.handles.forEach(w=>w.close()); this.handles=[]; clearTimeout(this.timer); clearInterval(this.poll); }
}
module.exports={scan,Watcher,norm,fingerprint,kind};
