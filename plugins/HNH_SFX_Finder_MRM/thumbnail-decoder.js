const path=require('node:path'),{pathToFileURL}=require('node:url');
// Decode in a separate renderer: a slow codec must not block the plugin UI or main process.
function createThumbnailDecoder({BrowserWindow,ipcMain,timeoutMs=6000}){
 let worker=null,pending=null,serial=0,disposed=false;
 function finish(value){if(!pending)return;const job=pending;pending=null;clearTimeout(job.timer);job.resolve(value);}
 function cancel(){finish(null);const old=worker;worker=null;if(old&&!old.isDestroyed())old.destroy();}
 function receive(event,result){if(!worker||event.sender!==worker.webContents||!pending||result?.id!==pending.id)return;
  const data=result.data;if(typeof data!=='string'||data.length>2800000||!data.startsWith('data:image/png;base64,')){finish(null);return;}
  const bytes=Buffer.from(data.slice(22),'base64');finish(bytes.length<=2*1024*1024?bytes:null);
 }
 ipcMain.on('thumbnail:complete',receive);
 async function create(file,kind){
  if(disposed)return null;if(pending)throw Error('Thumbnail decoder accepts one job at a time');
  const id=++serial;let resolve;const result=new Promise(r=>resolve=r);
  pending={id,resolve,timer:setTimeout(cancel,timeoutMs)};
  (async()=>{try{
   if(!worker){
    const win=new BrowserWindow({show:false,width:320,height:180,webPreferences:{preload:path.join(__dirname,'UI','thumbnail-preload.js'),contextIsolation:true,nodeIntegration:false,sandbox:true,webSecurity:false,partition:"hnh-thumbnails",backgroundThrottling:false}});
    // Only this sandboxed worker permits local file pixels on canvas. Its CSP blocks networking and external scripts.
    win.webContents.on('will-navigate',event=>event.preventDefault());
    win.webContents.on('new-window',event=>event.preventDefault());
    win.webContents.setWindowOpenHandler?.(()=>({action:'deny'}));
    worker=win;win.webContents.once('render-process-gone',()=>{if(worker===win)cancel();});
    win.once('closed',()=>{if(worker===win){worker=null;finish(null);}});
    await win.loadFile(path.join(__dirname,'UI','thumbnail-worker.html'));
   }
   if(pending?.id===id&&worker)worker.webContents.send('thumbnail:job',{id,url:pathToFileURL(file).href,kind});
  }catch{if(pending?.id===id)cancel();}})();
  return result;
 }
 return {create,cancel,dispose(){disposed=true;cancel();ipcMain.removeListener('thumbnail:complete',receive);}};
}
module.exports={createThumbnailDecoder};
