const fs=require('node:fs/promises');
function fpsValue(raw){const n=Number.parseFloat(raw);if(!Number.isFinite(n)||n<=0)throw Error('Không đọc được FPS timeline.');return n;}
function marksValue(raw){const out={};for(const type of ['audio','video']){out[type]={};for(const key of ['in','out']){const v=raw?.[type]?.[key];if(v!==undefined&&v!==null){const n=Number(v);if(Number.isInteger(n)&&n>=0)out[type][key]=n;}}}return out;}
function region(marks){const a=marks.audio,v=marks.video;const m=Object.keys(a).length?a:v;if(m.in===undefined||m.out===undefined||m.out<m.in)throw Error('Đánh đủ In và Out trên timeline Resolve trước khi chèn.');return {inFrame:m.in,outFrame:m.out,frames:m.out-m.in+1,markType:m===a?'audio':'video'};}
function sourceRange(start,duration,frames,timelineFps,sourceFps,outSeconds=duration){
 if(![start,duration,frames,timelineFps,sourceFps,outSeconds].every(Number.isFinite)||start<0||duration<=0||frames<1||timelineFps<=0||sourceFps<=0||outSeconds<=start||outSeconds>duration+.001)throw Error('Chọn file và chờ đọc thời lượng trước khi chèn.');
 const regionSeconds=frames/timelineFps,availableSeconds=Math.max(0,Math.min(duration,outSeconds)-start),seconds=Math.min(regionSeconds,availableSeconds);
 if(seconds<=0)throw Error('Không còn âm thanh sau điểm In đã chọn.');
 const startFrame=Math.round(start*sourceFps),sourceFrames=Math.max(1,Math.round(seconds*sourceFps)),expectedFrames=Math.max(1,Math.min(frames,Math.round(seconds*timelineFps)));
 return {startFrame,endFrame:startFrame+sourceFrames,expectedFrames,seconds:expectedFrames/timelineFps,limitedByTimeline:availableSeconds>regionSeconds+.001};
}
function registerTimelineRange({ipcMain,getResolve,loadIndex,importItem,resultItems,exclusive}){
 async function context(){const resolve=await getResolve();if(!resolve)throw Error('Chưa kết nối Resolve.');const manager=await resolve.GetProjectManager(),project=await manager.GetCurrentProject(),timeline=project&&await project.GetCurrentTimeline();if(!timeline)throw Error('Mở project và timeline trong Resolve.');if(typeof timeline.GetMarkInOut!=='function')throw Error('Resolve này chưa hỗ trợ đọc In/Out Timeline.');return {resolve,project,timeline};}
 async function snapshot(ctx){const t=ctx.timeline,marks=marksValue(await t.GetMarkInOut()),r=region(marks),fps=fpsValue(await t.GetSetting('timelineFrameRate')),start=Number(await t.GetStartFrame());if(!Number.isFinite(start))throw Error('Không đọc được điểm bắt đầu timeline.');return {...r,fps,start,recordFrame:start+r.inFrame,seconds:r.frames/fps,marks,id:String(await t.GetUniqueId()),name:String(await t.GetName())};}
 const same=(a,b)=>a.id===b.id&&a.start===b.start&&a.fps===b.fps&&JSON.stringify(a.marks)===JSON.stringify(b.marks);
 async function restore(t,marks){try{const current=marksValue(await t.GetMarkInOut());if(JSON.stringify(current)===JSON.stringify(marks))return '';if(Object.values(current).some(m=>Object.keys(m).length))return 'I/O đã thay đổi trong lúc chèn; giữ dấu mới của bạn.';for(const type of ['video','audio']){const m=marks[type];if(!Object.keys(m).length)continue;if(m.in===undefined||m.out===undefined||!await t.SetMarkInOut(m.in,m.out,type))return 'Đã chèn nhưng chưa khôi phục đầy đủ dấu I/O; kiểm tra timeline.';}return '';}catch{return 'Chưa khôi phục được dấu I/O; kiểm tra timeline.';}}
 ipcMain.handle('resolve:timeline-range',()=>exclusive(async()=>{try{return {ok:true,...await snapshot(await context())};}catch(e){return {ok:false,error:e.message};}}));
 ipcMain.handle('resolve:insert-range',(_e,request)=>exclusive(async()=>{
  let ctx,s,attempted=false,warning='',chosen={created:false};
  try{
   ctx=await context();s=await snapshot(ctx);const t=ctx.timeline;
   for(const method of ['GetIsTrackLocked','GetItemListInTrack','SetMarkInOut','DeleteClips'])if(typeof t[method]!=='function')throw Error('Resolve thiếu API '+method+' để chèn theo vùng.');
   let track=Number(request.trackIndex);const smart=request.audioOptions?.smart===true,count=Number(await t.GetTrackCount('audio'));if(!smart&&(!Number.isInteger(track)||track<1||track>count))throw Error('Audio track không còn tồn tại; tải lại track.');if(!smart&&await t.GetIsTrackLocked('audio',track))throw Error('Audio track đang khóa.');
   const index=await loadIndex(),file=index.files.find(f=>f.path===request.file?.path);if(!file||!['wav','mp3','flac','aiff','aif','m4a','aac','ogg'].includes(file.extension)||file.size!==request.file.size||file.mtimeMs!==request.file.mtimeMs)throw Error('Lựa chọn đã thay đổi; chọn lại file.');const stat=await fs.stat(file.path);if(stat.size!==file.size||stat.mtimeMs!==file.mtimeMs)throw Error('File đã thay đổi; quét lại thư viện.');
   // Check remaining duration before importing. The preview supplies decoded duration.
   sourceRange(request.inSeconds,request.duration,s.frames,s.fps,s.fps,request.outSeconds);
   const imported=await importItem(ctx.resolve,ctx.project,file.path);if(!imported.ok)throw Error(imported.error||'Không import được nhạc.');
   await require('./av-insert').audioSource(imported.item);
   let sourceFps=s.fps;try{const n=Number(await imported.item.GetClipProperty('FPS'));if(n>0&&Number.isFinite(n))sourceFps=n;}catch{}
   const range=sourceRange(request.inSeconds,request.duration,s.frames,s.fps,sourceFps,request.outSeconds),occupiedEnd=s.recordFrame+range.expectedFrames+2;
   const latest=await context();if(!same(s,await snapshot(latest)))throw Error('Timeline hoặc I/O vừa thay đổi. Kiểm tra vùng mới rồi bấm chèn lại.');
   if(smart){const guard=async()=>{if(!same(s,await snapshot(await context())))throw Error('Timeline hoặc I/O vừa thay đổi.');};chosen=await require('./smart-audio').chooseAudioTrack({timeline:t,preferred:track,start:s.recordFrame,end:occupiedEnd,create:request.audioOptions.create===true,placement:request.audioOptions.placement||'top',guard});track=chosen.trackIndex;await guard();if(!await require('./smart-audio').isFree(t,track,s.recordFrame,occupiedEnd))throw Error('Track vừa thay đổi.');}
   if(await t.GetIsTrackLocked('audio',track))throw Error('Audio track đang khóa.');
   for(const clip of resultItems(await t.GetItemListInTrack('audio',track))){const a=Number(await clip.GetStart()),b=Number(await clip.GetEnd());if(!Number.isFinite(a)||!Number.isFinite(b))throw Error('Không xác minh được vùng trống trên track.');if(a<occupiedEnd&&b>s.recordFrame)throw Error('Phần sẽ chèn trên A'+track+' đã có clip hoặc sát cuối đoạn (2 frame). Chọn track trống.');}
   const pool=await ctx.project.GetMediaPool();attempted=true;let clips;
   try{clips=resultItems(await pool.AppendToTimeline([{mediaPoolItem:imported.item,mediaType:2,trackIndex:track,recordFrame:s.recordFrame,startFrame:range.startFrame,endFrame:range.endFrame}]));}catch(e){throw Error('Resolve báo lỗi khi chèn; kiểm tra timeline trước khi thử lại. '+e.message);}
   let actualFrames;
   try{if(clips.length!==1)throw Error('Resolve không trả đúng một clip audio.');const c=clips[0],where=resultItems(await c.GetTrackTypeAndIndex()),start=Number(await c.GetStart());actualFrames=Number(await c.GetDuration());if(where[0]!=='audio'||Number(where[1])!==track||!Number.isFinite(start)||Math.abs(start-s.recordFrame)>.01||!Number.isFinite(actualFrames)||Math.abs(actualFrames-range.expectedFrames)>2)throw Error('Vị trí hoặc độ dài clip chưa khớp phần cần chèn.');}
   catch(e){let undone=false;if(clips.length)try{undone=!!await t.DeleteClips(clips,false);}catch{}throw Error(e.message+(undone?' Đã gỡ clip vừa tạo.':' Kiểm tra timeline trước khi thử lại.'));}
   warning=await restore(t,s.marks);attempted=false;
   return {ok:true,created:chosen.created,trackIndex:track,frames:actualFrames,requestedFrames:s.frames,inFrame:s.inFrame,outFrame:s.outFrame,seconds:actualFrames/s.fps,reused:imported.reused,limitedByTimeline:range.limitedByTimeline,warning:[chosen.warning,warning,actualFrames!==range.expectedFrames?'Resolve làm tròn lệch '+(actualFrames-range.expectedFrames)+' frame.':''].filter(Boolean).join(' ')};
  }catch(e){if(attempted&&ctx&&s)warning=await restore(ctx.timeline,s.marks);return {ok:false,error:e.message,warning:[chosen.warning,warning].filter(Boolean).join(' ')};}
 }));
}
module.exports={registerTimelineRange,marksValue,region,sourceRange};
