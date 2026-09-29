const bundled=require('./release-history.json');
const valid=v=>/^\d+\.\d+\.\d+$/.test(String(v));
function compare(a,b){const x=a.split('.').map(Number),y=b.split('.').map(Number);for(let i=0;i<3;i++)if(x[i]!==y[i])return x[i]-y[i];return 0;}
function between(from,to,extra=[]){const map=new Map();for(const r of [...bundled,...extra])if(valid(r?.version)&&compare(r.version,to)<=0&&(!valid(from)||compare(r.version,from)>0))map.set(r.version,{version:r.version,notes:String(r.notes||'Chưa có ghi chú cho phiên bản này.').slice(0,12000)});return [...map.values()].sort((a,b)=>compare(b.version,a.version));}
function text(entries){return entries.map(r=>'v'+r.version+'\n'+r.notes).join('\n\n────────────────────\n\n');}
module.exports={between,text,compare,valid};
