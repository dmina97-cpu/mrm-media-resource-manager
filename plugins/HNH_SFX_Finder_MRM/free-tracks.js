async function chooseTracks({timeline,videoTrack,audioTrack,start,end,resultItems,create=false}){
 if(!Number.isFinite(start)||!Number.isFinite(end)||end<=start)throw Error('Không xác định được vùng cần chèn.');
 for(const method of ['GetIsTrackLocked','GetItemListInTrack'])if(typeof timeline[method]!=='function')throw Error('Resolve thiếu API '+method+' để tìm track trống.');
 async function find(type,preferred){const count=Number(await timeline.GetTrackCount(type));if(!Number.isInteger(count)||count<0)throw Error('Không đọc được số track.');const order=[...new Set([Number(preferred),...Array.from({length:count},(_,i)=>i+1)])].filter(i=>Number.isInteger(i)&&i>0&&i<=count);for(const index of order){if(await timeline.GetIsTrackLocked(type,index))continue;let free=true;for(const clip of resultItems(await timeline.GetItemListInTrack(type,index))){const a=Number(await clip.GetStart()),b=Number(await clip.GetEnd());if(!Number.isFinite(a)||!Number.isFinite(b))throw Error('Không đọc được vùng clip trên '+type+' '+index);if(a<end&&b>start){free=false;break;}}if(free)return index;}return null;}
 let v=await find('video',videoTrack),a=await find('audio',audioTrack);const created=[];
 if((!v||!a)&&!create)throw Error('Không có đủ track hình/tiếng trống trong toàn bộ vùng chèn. Bật Tạo track khi cần hoặc chọn vị trí khác.');
 try{for(const type of ['video','audio']){if(type==='video'?v:a)continue;if(typeof timeline.AddTrack!=='function')throw Error('Resolve không hỗ trợ tạo track.');const ok=type==='audio'?await timeline.AddTrack('audio','stereo'):await timeline.AddTrack('video');if(!ok)throw Error('Không tạo được track '+type);const n=Number(await timeline.GetTrackCount(type));created.push((type==='video'?'V':'A')+n);if(type==='video')v=n;else a=n;}}catch(e){throw Error(e.message+(created.length?' Đã tạo track trống '+created.join(', ')+'.':''));}
 return {videoTrack:v,audioTrack:a,created};
}
module.exports={chooseTracks};
