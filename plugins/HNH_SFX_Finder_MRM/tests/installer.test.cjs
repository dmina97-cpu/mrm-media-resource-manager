const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'..');
const modules=fs.readdirSync(root).filter(n=>n.endsWith('.js'));
test('Windows installer copies every plugin module',()=>{
 const list=/\$items = @\(([^)]*)\)/.exec(fs.readFileSync(path.join(root,'install_windows.ps1'),'utf8'))[1].split(',').map(s=>s.trim().replace(/'/g,''));
 for(const name of modules)assert.ok(list.includes(name),name);
 assert.ok(list.includes('UI'));
});
test('macOS installer copies every plugin module',()=>{
 const list=/ITEMS="([^"]*)"/.exec(fs.readFileSync(path.join(root,'install_mac.sh'),'utf8'))[1].split(/\s+/);
 for(const name of modules)assert.ok(list.includes(name),name);
 assert.ok(list.includes('UI'));
});
