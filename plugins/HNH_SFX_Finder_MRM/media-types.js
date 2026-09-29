(function(root){
 const groups={audio:['wav','mp3','flac','aiff','aif','m4a','aac','ogg'],image:['jpg','jpeg','png','bmp','tif','tiff','webp','gif'],video:['mp4','mov','mxf','mkv','avi','m4v','webm']};
 const kind=extension=>Object.keys(groups).find(k=>groups[k].includes(String(extension||'').toLowerCase()))||null;
 const fileKind=file=>kind(file.extension||String(file.filename||file.path||'').split('.').pop());
 const api={groups,kind,fileKind,extensions:new Set(Object.values(groups).flat())};
 if(typeof module==='object'&&module.exports)module.exports=api;else root.HnhMediaTypes=api;
})(typeof window==='object'?window:globalThis);
