const {test}=require('node:test'),assert=require('node:assert/strict');
const S=require('../UI/shortcuts');
const ev=(code,m={})=>({code,ctrlKey:false,altKey:false,shiftKey:false,metaKey:false,...m});

test('key events become accelerators regardless of keyboard layout or Unikey',()=>{
 assert.equal(S.fromEvent(ev('Space')),'Space');
 assert.equal(S.fromEvent(ev('NumpadEnter',{shiftKey:true})),'Shift+Enter');
 assert.equal(S.fromEvent(ev('KeyI',{ctrlKey:true,altKey:true})),'Ctrl+Alt+I');
 assert.equal(S.fromEvent(ev('ArrowUp',{altKey:true})),'Alt+Up');
 assert.equal(S.fromEvent(ev('Enter',{metaKey:true})),'Ctrl+Enter');
 assert.equal(S.fromEvent(ev('Enter',{metaKey:true}),{mergeMeta:false}),'Cmd+Enter');
 assert.equal(S.fromEvent(ev('ControlLeft',{ctrlKey:true})),null);
});
test('defaults keep the old keys and add separate playhead and I/O inserts',()=>{
 const d=S.defaults();
 assert.equal(S.actionFor(d.local,'Space'),'play');
 assert.equal(S.actionFor(d.local,'Enter'),'insert');
 assert.equal(S.actionFor(d.local,'Ctrl+Enter'),'import');
 assert.equal(S.actionFor(d.local,'Shift+Enter'),'insertRange');
 assert.equal(S.actionFor(d.local,'Alt+Enter'),'insertPlayhead');
 assert.equal(d.globalEnabled,false);
});
test('plain keys can never be taken system-wide',()=>{
 for(const c of ['Space','Enter','A','Shift+A'])assert.equal(S.isValidGlobal(c),false,c);
 for(const c of ['Ctrl+Alt+Space','Alt+Enter','F13','Shift+F2'])assert.equal(S.isValidGlobal(c),true,c);
 assert.match(S.conflict('global',{},'play','Space'),/Ctrl hoặc Alt/);
});
test('conflicts and keys the plugin already uses are refused',()=>{
 const d=S.defaults();
 assert.match(S.conflict('local',d.local,'play','Enter'),/Chèn theo chế độ/);
 assert.match(S.conflict('local',d.local,'play','I'),/đã dùng sẵn/);
 assert.equal(S.conflict('local',d.local,'play','Space'),null);
 assert.equal(S.conflict('local',d.local,'play','K'),null);
});
test('normalize drops damaged bindings and keeps deliberate removals',()=>{
 const n=S.normalize({globalEnabled:true,local:{play:'K',insert:'not a key',import:null,insertRange:'I'},global:{play:'Space',next:'Ctrl+Alt+N',evil:'Ctrl+A'}});
 assert.equal(n.local.play,'K');assert.equal(n.local.insert,'Enter');assert.equal('import' in n.local,false);assert.equal(n.local.insertRange,'Shift+Enter');
 assert.equal(n.global.play,'Ctrl+Alt+Space');assert.equal(n.global.next,'Ctrl+Alt+N');assert.equal('evil' in n.global,false);assert.equal(n.globalEnabled,true);
});
test('only real text fields keep the keyboard',()=>{
 for(const type of ['range','checkbox'])assert.equal(S.isTextField({tagName:'INPUT',type}),false,type);
 for(const type of ['text','number','search'])assert.equal(S.isTextField({tagName:'INPUT',type}),true,type);
 assert.equal(S.isTextField({tagName:'SELECT'}),true);assert.equal(S.isTextField({tagName:'BUTTON'}),false);assert.equal(S.isTextField(null),false);
 assert.equal(S.allowedWhileTyping('Ctrl+Enter'),true);assert.equal(S.allowedWhileTyping('Alt+Enter'),true);assert.equal(S.allowedWhileTyping('Shift+Enter'),false);assert.equal(S.allowedWhileTyping('Space'),false);
});
