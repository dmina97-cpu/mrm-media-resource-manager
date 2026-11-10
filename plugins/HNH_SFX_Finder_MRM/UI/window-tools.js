(() => {
 const el=id=>document.getElementById(id),S=window.HnhShortcuts;
 const EDGE_LABEL={left:'Cạnh trái',right:'Cạnh phải',top:'Cạnh trên',bottom:'Cạnh dưới'};
 let current={pinned:false,dock:null,onTop:false,shortcuts:S.defaults(),failedShortcuts:[]},draft=null,capture=null;

 function runAction(action){
  if(document.body.dataset.mediaTab&&document.body.dataset.mediaTab!=='audio'){showStatus('Phím tắt SFX chỉ chạy ở tab Âm thanh.');return;}
  if(action==='play')el('playBtn').click();
  else if(action==='prev')moveSelection(-1);
  else if(action==='next')moveSelection(1);
  else if(action==='insert')insertSelected();
  else if(action==='insertPlayhead')insertSelected('playhead');
  else if(action==='insertRange')window.HnhTimelineRange.insert();
  else if(action==='import')importSelected();
 }
 // Trả về true nếu phím đã được xử lý.
 function handleLocalKey(e,typing){
  const combo=S.fromEvent(e),action=S.actionFor(current.shortcuts.local,combo);
  if(!action||(typing&&!S.allowedWhileTyping(combo)))return false;
  e.preventDefault();
  if(!e.repeat)runAction(action);
  return true;
 }
 const describe=combo=>combo||'—';
 function renderHeader(){
  document.body.dataset.dock=current.dock||'';
  el('pinBtn').setAttribute('aria-pressed',String(current.onTop));
  el('pinBtn').classList.toggle('active',current.onTop);
  el('pinBtn').disabled=!!current.dock;
  el('pinBtn').title=current.dock?'Khi dock, cửa sổ luôn nổi. Bỏ dock để tắt.':'Giữ cửa sổ luôn nổi trên DaVinci Resolve';
  el('dockLabel').textContent=current.dock?'Dock: '+EDGE_LABEL[current.dock]:'Dock';
  el('dockMenu').classList.toggle('active',!!current.dock);
  el('dockOff').hidden=!current.dock;
 }
 function renderSummary(){
  const local=current.shortcuts.local,rows=S.ACTIONS.filter(a=>a.local).map(a=>`<span>${describe(local[a.id])}: ${a.label}</span>`);
  el('shortcutSummary').innerHTML='<span>↑ / ↓: chọn SFX</span>'+rows.join('')+'<span>Ctrl+F: tìm kiếm</span><span>Esc: xóa tìm kiếm / đóng bảng</span>';
 }
 function apply(next){current=next;renderHeader();renderSummary();}
 function renderDialog(){
  const body=el('shortcutRows');body.textContent='';
  for(const action of S.ACTIONS){
   const row=document.createElement('tr'),name=document.createElement('td');name.textContent=action.label;row.append(name);
   for(const scope of ['local','global']){
    const cell=document.createElement('td');
    if(scope==='local'?!action.local:!action.global){cell.textContent=scope==='local'?action.fixedLocal||'—':'—';cell.className='shortcut-fixed';row.append(cell);continue;}
    const key=document.createElement('button'),listening=capture&&capture.action===action.id&&capture.scope===scope;
    key.type='button';key.className='secondary shortcut-key'+(listening?' listening':'');key.textContent=listening?'Nhấn phím…':describe(draft[scope][action.id]);
    key.onclick=()=>{capture={action:action.id,scope};el('shortcutError').textContent='';renderDialog();el('shortcutDialog').focus();};
    cell.append(key);row.append(cell);
   }
   body.append(row);
  }
  el('shortcutGlobalOn').checked=draft.globalEnabled;
 }
 function onCapture(e){
  if(!capture)return;
  e.preventDefault();e.stopPropagation();
  if(['ControlLeft','ControlRight','AltLeft','AltRight','ShiftLeft','ShiftRight','MetaLeft','MetaRight'].includes(e.code))return;
  const {action,scope}=capture;
  if(e.code==='Escape'){capture=null;el('shortcutError').textContent='';renderDialog();return;}
  if(e.code==='Backspace'&&!e.ctrlKey&&!e.altKey&&!e.shiftKey&&!e.metaKey){delete draft[scope][action];capture=null;renderDialog();return;}
  const combo=S.fromEvent(e,{mergeMeta:scope==='local'}),problem=combo?S.conflict(scope,draft[scope],action,combo):'Phím này không dùng được.';
  if(problem){el('shortcutError').textContent=(combo?combo+': ':'')+problem;return;}
  draft[scope][action]=combo;capture=null;el('shortcutError').textContent='';renderDialog();
 }
 const nullFill=bindings=>{const out={...bindings};for(const a of S.ACTIONS)if(!(a.id in out))out[a.id]=null;return out;};
 async function saveDialog(){
  const next=await window.hnh.setShortcuts({globalEnabled:el('shortcutGlobalOn').checked,local:nullFill(draft.local),global:nullFill(draft.global)});
  apply(next);
  if(next.failedShortcuts.length){el('shortcutError').textContent='Không đăng ký được (ứng dụng khác đang giữ): '+next.failedShortcuts.map(f=>f.combo).join(', ')+'. Đổi phím khác rồi Lưu lại.';return;}
  el('shortcutDialog').close();
 }

 el('pinBtn').onclick=async e=>{e.currentTarget.blur();apply(await window.hnh.setWindowPinned(!current.pinned));};
 for(const button of el('dockMenu').querySelectorAll('[data-dock-edge]'))button.onclick=async()=>{el('dockMenu').open=false;button.blur();apply(await window.hnh.setWindowDock(button.dataset.dockEdge||null));};
 document.addEventListener('pointerdown',e=>{if(el('dockMenu').open&&!el('dockMenu').contains(e.target))el('dockMenu').open=false;});
 el('dockMenu').querySelector('summary').addEventListener('click',e=>e.currentTarget.blur());
 const dialog=el('shortcutDialog');
 el('shortcutOpen').onclick=()=>{draft={...current.shortcuts,local:{...current.shortcuts.local},global:{...current.shortcuts.global}};capture=null;el('shortcutError').textContent='';renderDialog();dialog.showModal();};
 el('shortcutClose').onclick=()=>dialog.close();
 el('shortcutSave').onclick=saveDialog;
 el('shortcutReset').onclick=()=>{draft=S.defaults();capture=null;el('shortcutError').textContent='';renderDialog();};
 el('shortcutGlobalOn').onchange=e=>{draft.globalEnabled=e.target.checked;};
 dialog.addEventListener('keydown',e=>{e.stopPropagation();onCapture(e);});
 dialog.addEventListener('cancel',e=>{if(capture)e.preventDefault();});
 // Chọn xong trong danh sách thì trả phím về cho danh sách SFX.
 document.addEventListener('change',e=>{if(e.target.matches?.('main select,footer select'))e.target.blur();});

 window.hnh.onWindowModes(apply);
 window.hnh.onGlobalShortcut(runAction);
 window.HnhWindowTools={handleLocalKey,runAction};
 window.hnh.getWindowModes().then(apply);
})();
