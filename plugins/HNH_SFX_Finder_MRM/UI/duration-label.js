window.HnhDuration={
 format(value,clock=false){if(!Number.isFinite(value)||value<=0)return '—';if(!clock&&value<60)return value<.01?'<0.01s':value.toFixed(2)+'s';const total=Math.floor(value),hours=Math.floor(total/3600),minutes=Math.floor(total%3600/60),seconds=String(total%60).padStart(2,'0');return hours?hours+':'+String(minutes).padStart(2,'0')+':'+seconds:String(minutes).padStart(2,'0')+':'+seconds;},
 title(value){return Number.isFinite(value)&&value>0?'Thời lượng: '+value.toFixed(3)+' giây':'Chưa đọc được thời lượng; chọn file để thử xem/nghe.';}
};
