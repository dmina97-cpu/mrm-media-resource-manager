const trimState={in:0,out:null,active:false,drag:null,timer:null};
function previewDuration(){const d=$('#audio').duration;return Number.isFinite(d)&&d>0?d:(state.waveform.path===state.selected?.path?state.waveform.duration:0)||state.selected?.duration||0;}
function resetTrim(){trimState.in=0;trimState.out=null;trimState.active=false;clearTimeout(trimState.timer);renderTrim();schedulePreviewEnd();}
function trimRange(){const d=previewDuration();return {inSeconds:trimState.in,outSeconds:trimState.out??d,duration:d};}
function renderTrim(){
 const d=previewDuration(),r=trimRange(),valid=!!state.selected&&d>0;
 for(const id of ['markIn','markOut','inTime','outTime','resetTrim'])$('#'+id).disabled=!valid;
 $('#inTime').value=r.inSeconds.toFixed(3);$('#outTime').value=r.outSeconds.toFixed(3);
 $('#rangeLabel').textContent=valid?(trimState.active?`Đoạn chọn: ${(r.outSeconds-r.inSeconds).toFixed(3)}s`:`Toàn bộ: ${d.toFixed(3)}s`):'Chưa chọn SFX';
 $('#insertBtn').textContent=trimState.active?'+ Insert đoạn chọn':'+ Insert tại Playhead';
 window.HnhTimelineRange?.label();
 drawWaveform();
}
function setTrim(start,end){
 const d=previewDuration();if(!state.selected||!Number.isFinite(start)||!Number.isFinite(end)||start<0||end>d||end<=start){showStatus('Cần 0 ≤ In < Out ≤ thời lượng SFX.');renderTrim();return false;}
 trimState.in=start;trimState.out=end;trimState.active=start>0||end<d;renderTrim();const a=$('#audio');if(!a.paused&&trimState.active&&(a.currentTime<start||a.currentTime>=end))a.currentTime=start;schedulePreviewEnd();return true;
}
function playPreview(){const a=$('#audio');if(!state.selected)return;const r=trimRange();if(trimState.active&&(a.currentTime<r.inSeconds||a.currentTime>=r.outSeconds-0.001))a.currentTime=r.inSeconds;a.play().catch(e=>showStatus('Không phát được SFX: '+e.message));}
function previewBoundary(){
 const a=$('#audio'),r=trimRange();if(a.paused||!trimState.active)return;
 if(a.currentTime>=r.outSeconds-0.003){if($('#loopSelection').checked){a.currentTime=r.inSeconds;schedulePreviewEnd();}else{a.pause();a.currentTime=r.outSeconds;}}
}
function schedulePreviewEnd(){clearTimeout(trimState.timer);const a=$('#audio');if(!trimState.active||a.paused)return;const remaining=trimRange().outSeconds-a.currentTime;trimState.timer=setTimeout(()=>{previewBoundary();if(!a.paused)schedulePreviewEnd();},Math.max(10,remaining*1000));}
function drawTrimOverlay(ctx,w,h){
 const d=previewDuration();if(!d||!state.selected||!trimState.active)return;const r=trimRange(),left=r.inSeconds/d*w,right=r.outSeconds/d*w;
 ctx.fillStyle='#0009';ctx.fillRect(0,0,left,h);ctx.fillRect(right,0,w-right,h);
 ctx.fillStyle='#82b9f933';ctx.fillRect(left,0,right-left,h);ctx.fillStyle='#9acbff';ctx.fillRect(left,0,2,h);ctx.fillRect(Math.max(left,right-2),0,2,h);ctx.font=`${Math.max(10,Math.round(h/5))}px sans-serif`;ctx.fillText('I',left+4,12);ctx.fillText('O',Math.max(left+14,right-13),12);
}
function setupTrimControls(){
 $('#markIn').onclick=()=>setTrim($('#audio').currentTime,trimRange().outSeconds);
 $('#markOut').onclick=()=>setTrim(trimState.in,$('#audio').currentTime);
 $('#inTime').onchange=()=>setTrim(Number($('#inTime').value),trimRange().outSeconds);
 $('#outTime').onchange=()=>setTrim(trimState.in,Number($('#outTime').value));
 $('#resetTrim').onclick=resetTrim;
 $('#loopSelection').onchange=()=>{saveUiSoon();schedulePreviewEnd();};
 $('#previewVolume').oninput=()=>{applyPreviewVolume(Number($('#previewVolume').value),$('#previewMute').checked);saveUiSoon();};
 $('#previewMute').onchange=()=>{applyPreviewVolume(Number($('#previewVolume').value),$('#previewMute').checked);saveUiSoon();};
 const a=$('#audio');a.addEventListener('play',schedulePreviewEnd);a.addEventListener('pause',()=>clearTimeout(trimState.timer));a.addEventListener('seeked',schedulePreviewEnd);a.addEventListener('timeupdate',previewBoundary);
 a.addEventListener('ended',()=>{if(trimState.active&&$('#loopSelection').checked){a.currentTime=trimState.in;playPreview();}});
 const wrap=$('#waveWrap');const time=e=>{const rect=wrap.getBoundingClientRect();return Math.max(0,Math.min(previewDuration(),(e.clientX-rect.left)/rect.width*previewDuration()));};
 wrap.addEventListener('pointerdown',e=>{if(e.button!==0||!state.selected||!previewDuration())return;trimState.drag={x:e.clientX,time:time(e),moved:false,previous:{in:trimState.in,out:trimState.out,active:trimState.active}};a.pause();wrap.setPointerCapture(e.pointerId);});
 wrap.addEventListener('pointermove',e=>{const drag=trimState.drag;if(!drag)return;if(Math.abs(e.clientX-drag.x)>3)drag.moved=true;if(drag.moved){const t=time(e);if(t!==drag.time)setTrim(Math.min(t,drag.time),Math.max(t,drag.time));}});
 wrap.addEventListener('pointerup',e=>{const drag=trimState.drag;if(!drag)return;trimState.drag=null;if(drag.moved)a.currentTime=trimState.in;else a.currentTime=time(e);if(wrap.hasPointerCapture(e.pointerId))wrap.releasePointerCapture(e.pointerId);drawWaveform();});
 wrap.addEventListener('pointercancel',()=>{if(trimState.drag){Object.assign(trimState,trimState.drag.previous);trimState.drag=null;renderTrim();}});
 window.addEventListener('keydown',e=>{if(document.body.dataset.mediaTab&&document.body.dataset.mediaTab!=='audio')return;if(e.ctrlKey||e.metaKey||e.altKey||e.repeat||/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName)||!$('#libraryPanel').classList.contains('hidden'))return;const key=e.key.toLowerCase();if(key==='i'||key==='o'){e.preventDefault();$('#'+(key==='i'?'markIn':'markOut')).click();}});
 renderTrim();
}
function applyPreviewVolume(value,muted){const volume=Number.isFinite(value)?Math.max(0,Math.min(100,value)):100;$('#previewVolume').value=volume;$('#previewMute').checked=!!muted;$('#audio').volume=volume/100;$('#audio').muted=!!muted;$('#volumeLabel').textContent=volume+'%';}
