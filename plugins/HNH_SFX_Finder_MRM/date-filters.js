(function(root){
 const valid=n=>Number.isFinite(n)&&n>0;
 function day(value){if(!/^\d{4}-\d{2}-\d{2}$/.test(value||''))return null;const [y,m,d]=value.split('-').map(Number),date=new Date(y,m-1,d);return date.getFullYear()===y&&date.getMonth()===m-1&&date.getDate()===d?date:null;}
 function matches(addedAt,mode='all',from='',to='',now=new Date()){
  if(mode==='all')return true;if(mode==='unknown')return !valid(addedAt);if(!valid(addedAt))return false;
  let start,end;if(mode==='today'||mode==='week'){start=new Date(now.getFullYear(),now.getMonth(),now.getDate());if(mode==='week')start.setDate(start.getDate()-6);end=new Date(now.getFullYear(),now.getMonth(),now.getDate()+1);}
  else if(mode==='custom'){start=day(from);end=day(to);if((from&&!start)||(to&&!end))return false;if(end)end.setDate(end.getDate()+1);if(start&&end&&start>=end)return false;}else return false;
  return (!start||addedAt>=start.getTime())&&(!end||addedAt<end.getTime());
 }
 function label(n){return valid(n)?'Thêm vào thư viện: '+new Date(n).toLocaleString('vi-VN'):'Ngày thêm: chưa rõ (dữ liệu cũ)';}
 const api={valid,matches,label};if(typeof module==='object'&&module.exports)module.exports=api;else root.HnhDates=api;
})(typeof window==='object'?window:globalThis);
