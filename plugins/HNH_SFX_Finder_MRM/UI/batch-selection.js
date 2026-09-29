(() => {
 const el=id=>document.getElementById(id),selected=new Map(),lists=new Map(),tabSelections=new Map();
 let enabled=false,kind='audio',busy=false;
 const signature=f=>`${f.path}|${f.size}|${f.mtimeMs}`;
 function update(){el('batchCount').textContent=`Đã chọn ${selected.size} file`;for(const b of document.querySelectorAll('[data-batch-action]'))b.disabled=busy||!selected.size;el('batchClear').disabled=busy||!selected.size;el('batchPage').disabled=busy||!(lists.get(kind)?.page.length);el('batchToggle').disabled=busy;el('batchTags').disabled=busy;}
 function refresh(){render();window.HnhMedia?.batchRefresh();update();}
 function freeze(value){busy=value;for(const node of document.querySelectorAll('main,footer,#mediaWorkspace,#libraryPanel,[data-media-tab],#settingsBtn'))node.inert=value;for(const box of document.querySelectorAll(".batch-check input"))box.disabled=value;update();}
 el('batchToggle').onclick=()=>{enabled=!enabled;selected.clear();tabSelections.clear();el('batchBar').hidden=!enabled;el('batchToggle').setAttribute('aria-pressed',String(enabled));document.body.classList.toggle('batch-enabled',enabled);el('batchStatus').textContent='Chỉ chỉnh tags và yêu thích; file gốc không thay đổi.';refresh();};
 el('batchClear').onclick=()=>{selected.clear();refresh();};
 el('batchPage').onclick=()=>{for(const f of lists.get(kind)?.page||[])selected.set(f.path,f);refresh();};
 el('batchBar').addEventListener('keydown',e=>e.stopPropagation());
 for(const button of document.querySelectorAll('[data-batch-action]'))button.onclick=async()=>{
  if(busy||!selected.size)return;const files=[...selected.values()],operation=button.dataset.batchAction,tags=el('batchTags').value.split(',').map(t=>t.trim()).filter(Boolean);freeze(true);el('batchStatus').textContent='Đang lưu…';
  try{const result=await window.hnh.bulkMetadata({files,operation,tags});state.settings=result.settings;if(state.selected)el('tagsInput').value=(state.settings.tags?.[state.selected.path]||[]).join(', ');window.HnhMedia?.updated();refresh();el('batchStatus').textContent=`Đã cập nhật ${result.count} file.`;}catch(e){el('batchStatus').textContent=e.message||String(e);}finally{freeze(false);}
 };
 window.HnhBatch={
  tab(next){if(next!==kind){tabSelections.set(kind,new Map(selected));kind=next;selected.clear();for(const [path,file] of tabSelections.get(kind)||[])selected.set(path,file);el('batchStatus').textContent='';render();}update();},
  visible(type,page,all){lists.set(type,{page,all});if(type===kind){const valid=new Map(all.map(f=>[f.path,signature(f)]));for(const [path,f] of selected)if(valid.get(path)!==signature(f))selected.delete(path);update();}},
  decorate(container,file){if(!enabled)return;const label=document.createElement('label');label.className='batch-check';const box=document.createElement('input');box.type='checkbox';box.checked=selected.has(file.path);box.disabled=busy;box.setAttribute('aria-label','Chọn '+file.filename);container.classList.toggle('batch-picked',box.checked);label.append(box);for(const event of ['click','dblclick','keydown'])label.addEventListener(event,e=>e.stopPropagation());box.onchange=()=>{if(box.checked)selected.set(file.path,file);else selected.delete(file.path);container.classList.toggle('batch-picked',box.checked);update();};container.append(label);}
 };
})();
