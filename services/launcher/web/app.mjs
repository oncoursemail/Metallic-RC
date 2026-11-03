import { CATALOG, PLATFORMS } from '/catalog.mjs';
import { planInstallation } from '/planner.mjs';
const STORAGE_KEY='metallic.launcher.preferences.v1';
const components=CATALOG.filter(item=>item.kind==='component');
const allowedIds=new Set(components.map(item=>item.id)),selected=new Set();
let targetPlatform=null,restored=false;
try {
 const saved=JSON.parse(localStorage.getItem(STORAGE_KEY)||'null');
 if(saved?.schemaVersion===1&&Array.isArray(saved.selected)&&saved.selected.every(id=>allowedIds.has(id))){
  saved.selected.forEach(id=>selected.add(id));targetPlatform=PLATFORMS.includes(saved.platform)?saved.platform:null;restored=true;
 }
}catch{/* Missing or corrupt preferences leave every choice optional. */}
const byId=id=>document.getElementById(id);
const make=(tag,className,text)=>{const node=document.createElement(tag);if(className)node.className=className;if(text!==undefined)node.textContent=text;return node;};
const cards=new Map();
for(const item of components){
 const card=make('label','component'),top=make('span','component-top'),mark=make('span','component-mark',item.mark);
 mark.setAttribute('aria-hidden','true');
 const tag=make('span','optional-tag','OPTIONAL'),input=make('input');
 input.type='checkbox';input.id=item.id;input.setAttribute('aria-label',item.title);
 input.addEventListener('change',()=>{if(input.checked)selected.add(item.id);else selected.delete(item.id);byId('save-status').textContent='You have unsaved changes.';render();});
 top.append(mark,tag,input);
 const state=make('span','component-state','Not selected');
 card.append(top,make('span','component-name',item.title),make('span','component-description',item.description),make('span','component-detail',item.detail),state);
 byId('components').append(card);cards.set(item.id,{card,input,state});
}
byId('platform').value=targetPlatform||'';
byId('platform').addEventListener('change',event=>{targetPlatform=event.target.value||null;byId('save-status').textContent='You have unsaved changes.';render();});
byId('clear').addEventListener('click',()=>{selected.clear();byId('save-status').textContent='Selection cleared. Save to keep this choice.';render();});
byId('save').addEventListener('click',()=>{
 const choices=components.map(item=>item.id).filter(id=>selected.has(id));
 try {
  localStorage.setItem(STORAGE_KEY,JSON.stringify({schemaVersion:1,selected:choices,platform:targetPlatform}));
  byId('save-status').textContent=choices.length?'Saved '+choices.length+' component choice'+(choices.length===1?'':'s')+'. Installation remains pending.':'Saved an empty selection.';
 }catch{byId('save-status').textContent='This browser could not save your choices. They remain selected for this session.';}
});
function render(){
 for(const[id,{card,input,state}]of cards){input.checked=selected.has(id);card.classList.toggle('selected',input.checked);state.textContent=input.checked?'Selected':'Not selected';}
 byId('selection-count').textContent=selected.size+' selected';byId('clear').disabled=selected.size===0;
 const container=byId('plan');container.replaceChildren();
 try {
  // Preference storage is never an installed or downloaded inventory source.
  const plan=planInstallation({selected:[...selected],platform:targetPlatform});
  if(!plan.requested.length){container.append(make('p','empty-plan','Your setup is empty. Select a component to see its requirements.'));return;}
  const list=make('ol','plan-list');
  for(const operation of plan.operations){
   const row=make('li');row.append(make('strong','plan-title',operation.title));
   if(operation.kind==='runtime')row.append(make('span','dependency-label','Required by Minecraft Java'));
   row.append(make('p','plan-reason',operation.message||'Verified installation available.'));list.append(row);
  }
  container.append(list,make('p','plan-summary','Installation is pending the requirements above.'));
 }catch{container.append(make('p','plan-reason','The installation plan could not be validated. Review your choices and target platform.'));}
}
if(restored)byId('save-status').textContent='Your saved choices have been restored.';
render();
