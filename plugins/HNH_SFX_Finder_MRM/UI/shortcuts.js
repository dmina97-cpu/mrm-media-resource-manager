// Phím tắt: dùng chung cho giao diện (window.HnhShortcuts) và main process (require).
// Chuỗi phím theo cú pháp accelerator của Electron, vd "Ctrl+Alt+Space".
(root => {
 const ACTIONS=[
  {id:'play',label:'Nghe / dừng',local:'Space',global:'Ctrl+Alt+Space'},
  {id:'prev',label:'SFX trước',local:null,global:'Ctrl+Alt+Up',fixedLocal:'↑'},
  {id:'next',label:'SFX tiếp theo',local:null,global:'Ctrl+Alt+Down',fixedLocal:'↓'},
  {id:'insert',label:'Chèn theo chế độ đang chọn',local:'Enter',global:'Ctrl+Alt+Enter'},
  {id:'insertPlayhead',label:'Chèn tại playhead',local:'Alt+Enter',global:'Ctrl+Alt+P'},
  {id:'insertRange',label:'Chèn vào đoạn I/O timeline',local:'Shift+Enter',global:'Ctrl+Alt+I'},
  {id:'import',label:'Import vào Media Pool',local:'Ctrl+Enter',global:null}
 ];
 // Các phím plugin đã dùng cố định trong cửa sổ.
 const RESERVED=new Set(['Up','Down','I','O','Esc','Ctrl+F']);
 const NAMED={Space:'Space',Enter:'Enter',NumpadEnter:'Enter',Tab:'Tab',Escape:'Esc',Backspace:'Backspace',Delete:'Delete',Insert:'Insert',Home:'Home',End:'End',PageUp:'PageUp',PageDown:'PageDown',ArrowUp:'Up',ArrowDown:'Down',ArrowLeft:'Left',ArrowRight:'Right',Minus:'-',Equal:'=',BracketLeft:'[',BracketRight:']',Semicolon:';',Quote:"'",Comma:',',Period:'.',Slash:'/',Backslash:'\\',Backquote:'`'};
 const MODIFIERS=['Ctrl','Cmd','Alt','Shift'];
 const KEY=/^(?:[A-Z0-9]|F(?:[1-9]|1[0-9]|2[0-4])|num[0-9]|Space|Enter|Tab|Esc|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Up|Down|Left|Right|[-=[\];',./\\`])$/;

 // Theo e.code để Unikey / kiểu bàn phím không đổi kết quả.
 function keyName(code){
  if(!code)return null;
  if(NAMED[code])return NAMED[code];
  let m=/^Key([A-Z])$/.exec(code)||/^Digit([0-9])$/.exec(code);
  if(m)return m[1];
  if(/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(code))return code;
  m=/^Numpad([0-9])$/.exec(code);
  return m?'num'+m[1]:null;
 }
 function fromEvent(e,{mergeMeta=true}={}){
  const key=keyName(e.code);
  if(!key)return null;
  const parts=[];
  if(e.ctrlKey||(mergeMeta&&e.metaKey))parts.push('Ctrl');
  if(!mergeMeta&&e.metaKey)parts.push('Cmd');
  if(e.altKey)parts.push('Alt');
  if(e.shiftKey)parts.push('Shift');
  parts.push(key);
  return parts.join('+');
 }
 function parse(value){
  if(typeof value!=='string'||!value)return null;
  const parts=value==='+'?[value]:value.split('+'),key=parts.pop(),mods=parts;
  if(!KEY.test(key)||new Set(mods).size!==mods.length||!mods.every(m=>MODIFIERS.includes(m)))return null;
  return {key,mods};
 }
 const isValid=value=>!!parse(value);
 // Phím toàn hệ thống phải có Ctrl/Alt/Cmd (hoặc là F1–F24), nếu không sẽ cướp phím gõ của mọi ứng dụng.
 function isValidGlobal(value){const p=parse(value);return !!p&&(p.mods.some(m=>m!=='Shift')||/^F\d/.test(p.key));}
 function defaults(){const local={},global={};for(const a of ACTIONS){if(a.local)local[a.id]=a.local;if(a.global)global[a.id]=a.global;}return {local,global,globalEnabled:false};}
 function normalize(raw){
  const out=defaults();
  if(!raw||typeof raw!=='object')return out;
  out.globalEnabled=raw.globalEnabled===true;
  for(const scope of ['local','global']){
   const given=raw[scope];
   if(!given||typeof given!=='object')continue;
   const valid=scope==='local'?isValid:isValidGlobal;
   for(const a of ACTIONS){
    if(scope==='local'?!a.local:!a.global)continue;
    if(!Object.prototype.hasOwnProperty.call(given,a.id))continue;
    if(given[a.id]===null)delete out[scope][a.id];
    else if(valid(given[a.id])&&!(scope==='local'&&RESERVED.has(given[a.id])))out[scope][a.id]=given[a.id];
   }
  }
  return out;
 }
 function actionFor(bindings,combo){if(!combo)return null;for(const [id,value] of Object.entries(bindings||{}))if(value===combo)return id;return null;}
 // Lý do không gán được, hoặc null nếu được.
 function conflict(scope,bindings,action,combo){
  if(scope==='global'?!isValidGlobal(combo):!isValid(combo))return scope==='global'?'Phím dùng ngoài plugin phải kèm Ctrl hoặc Alt (hoặc là F1–F24).':'Phím này không dùng được.';
  if(scope==='local'&&RESERVED.has(combo))return 'Phím này plugin đã dùng sẵn.';
  const other=actionFor(bindings,combo);
  if(other&&other!==action)return 'Đã gán cho: '+ACTIONS.find(a=>a.id===other).label+'.';
  return null;
 }
 function isTextField(node){
  if(!node)return false;
  if(node.tagName==='TEXTAREA'||node.tagName==='SELECT'||node.isContentEditable)return true;
  return node.tagName==='INPUT'&&!['range','checkbox','radio','button'].includes(String(node.type).toLowerCase());
 }
 // Phím không kèm Ctrl/Alt thì nhường cho ô đang gõ.
 const allowedWhileTyping=combo=>/^(?:Ctrl|Cmd|Alt)\+/.test(combo||'');
 const api={ACTIONS,fromEvent,isValid,isValidGlobal,defaults,normalize,actionFor,conflict,isTextField,allowedWhileTyping};
 if(typeof module==='object'&&module.exports)module.exports=api;else root.HnhShortcuts=api;
})(typeof window==='undefined'?globalThis:window);
