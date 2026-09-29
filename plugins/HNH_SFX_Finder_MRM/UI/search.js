(function(root){
const VI_ALIASES = {
  'tieng': '', 'am thanh': '',
  'chuyen canh': 'transition whoosh swoosh swish', 'vuot qua': 'passby whoosh', 'gio': 'wind air whoosh',
  'nhanh': 'fast quick short', 'cham': 'slow long', 'ngan': 'short quick', 'dai': 'long',
  'manh': 'heavy hard strong big', 'nhe': 'soft light subtle', 'tram': 'low bass deep', 'cao': 'high bright',
  'dam': 'punch hit impact body hit', 'danh': 'hit impact punch', 'va cham': 'impact hit collision',
  'kim loai': 'metal metallic clang clank', 'go': 'wood wooden knock', 'kinh': 'glass shatter break',
  'roi': 'drop fall', 'vo': 'break smash crack shatter', 'no': 'explosion boom blast',
  'bam': 'click tap press', 'chuot': 'mouse click', 'thong bao': 'notification alert ping',
  'xe may': 'motorcycle motorbike engine bike', 'o to': 'car vehicle engine', 'dong co': 'engine motor',
  'phanh': 'brake skid', 'lop': 'tire tyre', 'cua': 'door', 'mo': 'open', 'dong': 'close shut',
  'buoc chan': 'footstep footsteps walk', 'chay': 'run running', 'cuoi': 'laugh laughter', 'vo tay': 'clap applause',
  'nuoc': 'water splash', 'lua': 'fire flame', 'mua': 'rain', 'sam': 'thunder', 'dien': 'electric electricity zap',
  'ma thuat': 'magic magical', 'cang thang': 'tension suspense dark cinematic', 'hoanh trang': 'epic cinematic big',
  'tang dan': 'riser rise build up', 'giam dan': 'downer fall descend', 'pop': 'pop plop', 'click': 'click tick tap'
};

const SYNONYMS = {
  whoosh: ['swoosh','swish','transition','passby','air'], swoosh:['whoosh','swish','transition'],
  impact:['hit','slam','thump','boom','punch'], hit:['impact','slam','punch','thump'], boom:['impact','bass','cinematic','hit'],
  click:['tap','tick','ui','button'], pop:['ui','bubble','plop'], riser:['rise','build','uplifter','swell'],
  downer:['fall','down','descend','drop'], metal:['metallic','clang','clank','steel','iron'],
  glass:['shatter','break','crack'], wood:['wooden','knock','hit'], car:['vehicle','auto','engine'],
  motorcycle:['motorbike','bike','engine'], fast:['quick','short','speed'], soft:['light','subtle','gentle'],
  heavy:['big','hard','strong','bass'], bass:['low','deep','sub']
};

function norm(s=''){return String(s).normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/[đĐ]/g,'d').toLowerCase().replace(/[_\-.()[\]{}]+/g,' ').replace(/\s+/g,' ').trim();}
function distance(a,b){if(Math.abs(a.length-b.length)>2)return 99;let row=Array.from({length:b.length+1},(_,i)=>i);for(let i=1;i<=a.length;i++){let next=[i];for(let j=1;j<=b.length;j++)next[j]=Math.min(next[j-1]+1,row[j]+1,row[j-1]+(a[i-1]===b[j-1]?0:1));row=next;}return row[b.length];}
function compile(raw){
 const words=norm(raw).split(' ').filter(Boolean),groups=[];
 const phrases=Object.keys(VI_ALIASES).sort((a,b)=>b.split(' ').length-a.split(' ').length);
 for(let i=0;i<words.length;){
  const phrase=phrases.find(k=>words.slice(i,i+k.split(' ').length).join(' ')===k);
  const original=phrase||words[i];i+=phrase?phrase.split(' ').length:1;
  if(phrase && !VI_ALIASES[phrase])continue;
  const alternatives=new Set([original,...(VI_ALIASES[phrase]||'').split(' ').filter(Boolean)]);
  for(const t of [...alternatives])for(const v of SYNONYMS[t]||[])alternatives.add(v);
  groups.push({original,alternatives:[...alternatives]});
 }
 return {query:norm(raw),groups};
}
const textCache=new WeakMap();
function score(file,query,tags=[]){
 if(!query.query)return 1;
 const tagKey=tags.join('\0');let entry=textCache.get(file);if(!entry||entry.source!==file.basename||entry.folders!==file.folders||entry.tagKey!==tagKey){const base=norm(file.basename),full=norm([file.basename,...file.folders||[],...tags].join(' '));entry={source:file.basename,folders:file.folders,tagKey,base,full,words:full.split(' ')};textCache.set(file,entry);}const {base,full,words}=entry;
 let total=0,matched=0;
 for(const group of query.groups){let best=0;
  for(const term of group.alternatives){
   let v=0;if((' '+full+' ').includes(' '+term+' '))v=(' '+base+' ').includes(' '+term+' ')?12:9;
   else if(term.length>=3 && words.some(w=>w.startsWith(term)))v=6;
   else if(term.length>=4 && words.some(w=>w.length>=4 && distance(term,w)<=(term.length>=7?2:1)))v=4;
   if(term===group.original)v*=1.4;best=Math.max(best,v);
  }
  if(best){matched++;total+=best;}
 }
 if(!matched)return 0;
 const coverage=matched/Math.max(1,query.groups.length);
 return total*coverage*coverage+(base===query.query?80:base.includes(query.query)?30:0);
}
function durationMatches(duration,min,max){if(min===''&&max==='')return true;return Number.isFinite(duration)&&(min===''||duration>=Number(min))&&(max===''||duration<=Number(max));}
const api={norm,compile,score,durationMatches};if(typeof module!=='undefined')module.exports=api;else root.HnhSearch=api;
})(typeof window!=='undefined'?window:globalThis);
