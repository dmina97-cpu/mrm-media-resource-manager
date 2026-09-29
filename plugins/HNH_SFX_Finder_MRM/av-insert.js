// Only the clips returned by this append may be removed on a failed transaction.
async function audioSource(item){
 if(typeof item.GetAudioMapping==='function'){
  let mapping;try{const raw=await item.GetAudioMapping();mapping=typeof raw==='string'?JSON.parse(raw):raw;}catch{}
  if(mapping&&typeof mapping==='object'){
   if(Object.keys(mapping.linked_audio||{}).length)throw Error('Clip có audio liên kết ngoài. Hãy Import và chèn bằng Resolve để giữ mapping hiện tại.');
   if(Number(mapping.embedded_audio_channels)===0)throw Error('Video không có âm thanh. Bỏ chọn Kèm âm thanh để chèn phần hình.');
   const tracks=Object.values(mapping.track_mapping||{});if(tracks.length>1)throw Error('Video có nhiều track âm thanh nguồn. Bản này hỗ trợ một track nguồn; chỉnh Clip Attributes hoặc chèn bằng Resolve.');
   if(tracks[0]?.mute)throw Error('Track nguồn đang mute trong Clip Attributes. Hãy kiểm tra trước khi chèn.');
   if(Number(mapping.embedded_audio_channels)>0)return;
  }
 }
 let channels;try{channels=await item.GetClipProperty('Audio Ch');}catch{}
 if(!/^\d+$/.test(String(channels??'').trim())||Number(channels)<=0)throw Error('Không xác nhận được audio nguồn. Kiểm tra Clip Attributes hoặc bỏ chọn Kèm âm thanh.');
 // Without mapping, allow ordinary mono/stereo only.
 if(Number(channels)>2)throw Error('Nguồn nhiều kênh cần kiểm tra Audio Mapping trong Resolve trước khi chèn.');
}
async function appendAV({timeline,pool,item,range,pos,sourceFps,videoTrack,audioTrack,resultItems}){
 if(!Number.isFinite(sourceFps)||sourceFps<=0)throw Error('Không đọc được FPS nguồn để đồng bộ hình và tiếng.');
 for(const method of ['GetItemListInTrack','SetClipsLinked','DeleteClips'])if(typeof timeline[method]!=='function')throw Error('Resolve chưa cung cấp '+method+' để chèn hình/tiếng an toàn.');
 const count=Number(await timeline.GetTrackCount('audio'));
 if(!Number.isInteger(audioTrack)||audioTrack<1||audioTrack>count)throw Error('Chọn audio track còn tồn tại trong timeline.');
 if(typeof timeline.GetIsTrackLocked!=='function')throw Error('Không đọc được trạng thái khóa audio track.');
 if(await timeline.GetIsTrackLocked('video',videoTrack))throw Error('Video track đang khóa.');
 if(await timeline.GetIsTrackLocked('audio',audioTrack))throw Error('Audio track đang khóa. Chọn track khác hoặc mở khóa.');
 await audioSource(item);
 const duration=(range.endFrame-range.startFrame+1)/sourceFps*pos.fps,end=pos.recordFrame+Math.ceil(duration)+2;
 for(const [type,index] of [['video',videoTrack],['audio',audioTrack]]){
  for(const clip of resultItems(await timeline.GetItemListInTrack(type,index))){const start=Number(await clip.GetStart()),stop=Number(await clip.GetEnd());if(!Number.isFinite(start)||!Number.isFinite(stop))throw Error('Không xác minh được vùng trống trên track.');if(start<end&&stop>pos.recordFrame)throw Error('Vùng chèn trên '+(type==='audio'?'A':'V')+index+' đã có clip. Chọn track trống hoặc vị trí khác để chèn hình và tiếng.');}
 }
 let clips;
 try{clips=resultItems(await pool.AppendToTimeline([{mediaPoolItem:item,mediaType:1,trackIndex:videoTrack,recordFrame:pos.recordFrame,...range},{mediaPoolItem:item,mediaType:2,trackIndex:audioTrack,recordFrame:pos.recordFrame,...range}]));}
 catch(e){throw Error('Resolve báo lỗi khi chèn: '+(e.message||e)+'. Có thể đã chèn một phần; kiểm tra timeline trước khi thử lại.');}
 try{
  if(clips.length!==2)throw Error('Resolve không trả đủ một clip hình và một clip tiếng.');
  const seen=new Map();for(const clip of clips){const where=resultItems(await clip.GetTrackTypeAndIndex()),type=where[0],index=Number(where[1]),start=Number(await clip.GetStart()),length=Number(await clip.GetDuration());if(!['audio','video'].includes(type)||seen.has(type)||index!==(type==='audio'?audioTrack:videoTrack)||!Number.isFinite(start)||!Number.isFinite(length)||Math.abs(start-pos.recordFrame)>.01||Math.abs(length-duration)>2.01)throw Error('Clip '+type+' trên track '+index+': bắt đầu '+start+', dài '+length+' frame; cần track '+(type==='audio'?audioTrack:videoTrack)+', bắt đầu '+pos.recordFrame+', dài '+duration.toFixed(3)+' frame (FPS nguồn '+sourceFps+', timeline '+pos.fps+').');seen.set(type,{start,length});}
  if(Math.abs(seen.get('video').length-seen.get('audio').length)>2.01)throw Error('Hình/tiếng không đồng bộ: video '+seen.get('video').length+' frame, audio '+seen.get('audio').length+' frame.');
  if(!await timeline.SetClipsLinked(clips,true))throw Error('Không liên kết được clip hình và tiếng.');
  const rounded=Math.abs(seen.get('video').length-duration)>1.01||Math.abs(seen.get('audio').length-duration)>1.01||Math.abs(seen.get('video').length-seen.get('audio').length)>1.01;return {linked:true,audioTrackIndex:audioTrack,...(rounded?{roundingWarning:'Resolve làm tròn hình/tiếng lệch tối đa 2 frame.'}:{})};
 }catch(e){let undone=false;if(clips.length)try{undone=!!await timeline.DeleteClips(clips,false);}catch{}throw Error(e.message+(undone?' Đã gỡ các clip vừa chèn; clip cũ không bị xóa.':clips.length?' Chưa gỡ được các clip vừa chèn; kiểm tra timeline trước khi thử lại.':' Kiểm tra timeline trước khi thử lại.'));}
}
module.exports={appendAV,audioSource};
