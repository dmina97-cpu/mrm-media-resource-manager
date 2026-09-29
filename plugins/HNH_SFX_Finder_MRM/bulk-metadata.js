function applyBulk(settings,index,request){
 const operations=['addTags','removeTags','favorite','unfavorite'];if(!request||!operations.includes(request.operation)||!Array.isArray(request.files)||!request.files.length||request.files.length>10000)throw Error('Lựa chọn hoặc thao tác không hợp lệ.');
 const current=new Map(index.files.map(f=>[f.path,f])),selected=new Map();for(const file of request.files){const found=current.get(file.path);if(!found||found.size!==file.size||found.mtimeMs!==file.mtimeMs)throw Error('Danh sách đã thay đổi. Chọn lại file trước khi áp dụng.');selected.set(file.path,found);}
 const normalize=t=>t.normalize('NFC').toLocaleLowerCase('vi');let input=[];
 if(request.operation.endsWith('Tags')){if(!Array.isArray(request.tags)||request.tags.some(t=>typeof t!=='string'||t.trim().length>160))throw Error('Tags không hợp lệ hoặc dài quá 160 ký tự.');input=[...new Set(request.tags.map(t=>t.trim()).filter(Boolean))];if(!input.length||input.length>30)throw Error('Nhập từ 1 đến 30 tag, cách nhau bằng dấu phẩy.');}
 const tags={...(settings.tags||{})},favorites=new Set(settings.favorites||[]),wanted=new Set(input.map(normalize));
 for(const file of selected.values()){
  if(request.operation==='favorite')favorites.add(file.path);
  else if(request.operation==='unfavorite')favorites.delete(file.path);
  else if(request.operation==='removeTags')tags[file.path]=(tags[file.path]||[]).filter(t=>!wanted.has(normalize(t)));
  else{const list=[...(tags[file.path]||[])],existing=new Set(list.map(normalize));for(const tag of input)if(!existing.has(normalize(tag))){existing.add(normalize(tag));list.push(tag);}if(list.length>30)throw Error(file.filename+' sẽ vượt 30 tags; chưa áp dụng thay đổi cho file nào.');tags[file.path]=list;}
 }
 return {settings:{...settings,tags,favorites:[...favorites]},count:selected.size};
}
module.exports={applyBulk};
