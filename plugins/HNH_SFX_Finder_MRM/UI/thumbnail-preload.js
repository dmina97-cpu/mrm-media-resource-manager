const {contextBridge,ipcRenderer}=require('electron');
contextBridge.exposeInMainWorld('thumbnailWorker',{
 onJob:fn=>ipcRenderer.on('thumbnail:job',(_event,job)=>fn(job)),
 complete:result=>ipcRenderer.send('thumbnail:complete',result)
});
