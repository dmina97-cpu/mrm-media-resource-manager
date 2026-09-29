// Build a folder tree from the existing SFX index, without moving any files.
let folderSelection=null,folderCacheFiles=null,folderRoots=[],folderOpen=new Map();
document.querySelector('#folderTree').addEventListener('keydown',event=>{
 if(['Enter',' ','ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End'].includes(event.key))event.stopPropagation();
});
function folderParts(file){return String(file.relativePath||file.filename).replace(/\\/g,'/').split('/').slice(0,-1).filter(p=>p&&p!=='.');}
function folderKey(node){return JSON.stringify([node.root,node.path]);}
function matchesFolder(file){if(!folderSelection)return true;const p=folderParts(file).join('/');return file.root===folderSelection.root&&(!folderSelection.path||p===folderSelection.path||p.startsWith(folderSelection.path+'/'));}
function buildFolderTree(){
 if(folderCacheFiles===state.files)return;folderCacheFiles=state.files;const roots=new Map();
 for(const file of state.files){let node=roots.get(file.root);if(!node){node={root:file.root,path:'',name:file.root.split(/[\\/]/).filter(Boolean).pop()||file.root,count:0,children:new Map()};roots.set(file.root,node);}node.count++;const parts=[];for(const part of folderParts(file)){parts.push(part);let child=node.children.get(part);if(!child){child={root:file.root,path:parts.join('/'),name:part,count:0,children:new Map()};node.children.set(part,child);}child.count++;node=child;}}
 folderRoots=[...roots.values()].sort((a,b)=>a.name.localeCompare(b.name));
 if(folderSelection&&!state.files.some(matchesFolder))folderSelection=null;
}
function chooseFolder(node){folderSelection=node?{root:node.root,path:node.path}:null;$('#libraryFilter').value=node?node.root:'all';render();saveUiSoon();}
function renderFolders(){
 buildFolderTree();const visible=$('#showFolders').checked;$('#folderBrowser').hidden=!visible;$('#browserArea').classList.toggle('with-folders',visible);
 // Show the active folder even when the sidebar is collapsed.
 $('#folderCurrent').textContent=folderSelection?'Thư mục: '+(folderSelection.path||folderSelection.root):'Mọi thư mục';
 $('#clearFolder').hidden=!folderSelection;
 if(!visible)return;
 const host=$('#folderTree');host.textContent='';const all=document.createElement('button');all.className='folder-select'+(!folderSelection?' active':'');all.textContent='Tất cả SFX';all.onclick=()=>chooseFolder(null);host.appendChild(all);
 function append(node,parent){const key=folderKey(node),children=[...node.children.values()].sort((a,b)=>a.name.localeCompare(b.name));let container=parent;
  const button=document.createElement('button');button.className='folder-select'+(folderSelection?.root===node.root&&folderSelection?.path===node.path?' active':'');button.title=node.root+(node.path?' / '+node.path:'');button.dataset.folder=key;button.textContent=node.name+' ('+node.count+')';button.onclick=e=>{e.preventDefault();e.stopPropagation();chooseFolder(node);};
  if(children.length){const detail=document.createElement('details'),summary=document.createElement('summary');detail.open=folderOpen.has(key)?folderOpen.get(key):(!node.path||(folderSelection?.root===node.root&&folderSelection.path.startsWith(node.path+'/')));detail.ontoggle=()=>{if(detail.isConnected)folderOpen.set(key,detail.open);};summary.appendChild(button);detail.appendChild(summary);container=document.createElement('div');container.className='folder-children';detail.appendChild(container);parent.appendChild(detail);}else{const row=document.createElement('div');row.className='folder-row';row.appendChild(button);parent.appendChild(row);}
  for(const child of children)append(child,container);
 }
 for(const root of folderRoots)append(root,host);
 if(!folderRoots.length){const empty=document.createElement('p');empty.textContent='Thêm thư viện để duyệt thư mục.';host.appendChild(empty);}
}
