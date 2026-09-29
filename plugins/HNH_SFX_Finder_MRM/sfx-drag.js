const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
function registerSfxDrag({ipcMain,loadIndex,nativeImage,getWindow}){
 const prepared=new Map();
 ipcMain.handle('sfx:prepare-drag',async(event,input)=>{try{
  if(event.sender!==getWindow()?.webContents)throw Error('Cửa sổ không hợp lệ.');const index=await loadIndex(),file=index.files.find(f=>f.path===input?.path);
  if(!file||!path.isAbsolute(file.path)||!['wav','mp3','flac','aiff','aif','m4a','aac','ogg'].includes(file.extension)||file.size!==input.size||file.mtimeMs!==input.mtimeMs)throw Error('Chọn lại SFX còn trong thư viện.');const stat=await fs.stat(file.path);if(!stat.isFile()||stat.size!==file.size||stat.mtimeMs!==file.mtimeMs)throw Error('File đã thay đổi hoặc không truy cập được.');
  const token=crypto.randomUUID();prepared.set(token,{path:file.path,sender:event.sender,expires:Date.now()+15000});for(const [key,v] of prepared)if(v.expires<Date.now()||prepared.size>20)prepared.delete(key);return {ok:true,token};
 }catch(e){return {ok:false,error:e.message};}});
 ipcMain.on('sfx:start-drag',(event,token)=>{const entry=prepared.get(token);prepared.delete(token);if(!entry||entry.sender!==event.sender||event.sender!==getWindow()?.webContents||entry.expires<Date.now())return;
  try{const icon=nativeImage.createFromPath(path.join(__dirname,'UI','sfx-drag.png'));if(icon.isEmpty())throw Error('Không đọc được biểu tượng kéo thả.');event.sender.startDrag({file:entry.path,icon});}catch(e){event.sender.send('sfx:drag-error',e.message);}
 });
}
module.exports={registerSfxDrag};
