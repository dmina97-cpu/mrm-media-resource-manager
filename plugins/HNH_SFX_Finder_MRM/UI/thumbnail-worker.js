window.thumbnailWorker.onJob(job=>{
 const video=job.kind==='video',element=document.createElement(video?'video':'img');let done=false;
 function finish(data){if(done)return;done=true;element.onload=element.onerror=element.onloadeddata=element.onseeked=null;if(video){element.pause();element.removeAttribute('src');element.load();}else element.removeAttribute('src');window.thumbnailWorker.complete({id:job.id,data});}
 function draw(){try{const w=video?element.videoWidth:element.naturalWidth,h=video?element.videoHeight:element.naturalHeight;if(!w||!h)return finish(null);const scale=Math.min(320/w,180/h,1),canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(w*scale));canvas.height=Math.max(1,Math.round(h*scale));canvas.getContext('2d').drawImage(element,0,0,canvas.width,canvas.height);finish(canvas.toDataURL('image/png'));}catch{finish(null);}}
 element.onerror=()=>finish(null);
 if(video){element.muted=true;element.preload='auto';element.onloadeddata=()=>{element.onloadeddata=null;const target=Math.min(1,element.duration/10);if(Number.isFinite(target)&&target>.01){element.onseeked=draw;try{element.currentTime=target;}catch{draw();}}else draw();};}else element.onload=draw;
 element.src=job.url;
});
