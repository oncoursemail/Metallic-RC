import { CATALOG, PLATFORMS } from './catalog.mjs';
const fail = message => { throw new TypeError(message); };
const validId = value => typeof value === 'string' && /^[a-z][a-z0-9-]{0,63}$/.test(value);
const validVersion = value => typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/.test(value);
const validHash = value => typeof value === 'string' && /^[a-fA-F0-9]{64}$/.test(value);
function indexCatalog(catalog) {
  if (!Array.isArray(catalog)) fail('Catalog must be an array.');
  const index = new Map();
  for (const item of catalog) {
    if (!item || !validId(item.id) || index.has(item.id)) fail('Catalog IDs must be valid and unique.');
    if (!['component','runtime'].includes(item.kind) || typeof item.title !== 'string' || !item.title.trim()) fail('Invalid catalog item.');
    if (!Array.isArray(item.requires) || new Set(item.requires).size !== item.requires.length) fail('Dependencies must be a unique array.');
    if (item.distribution != null && (typeof item.distribution !== 'object' || Array.isArray(item.distribution))) fail('Invalid distribution manifest.');
    index.set(item.id,item);
  }
  for (const item of index.values()) {
    for (const id of item.requires) {
      if (!index.has(id)) fail('Unknown dependency: '+id);
      if (index.get(id).kind !== 'runtime') fail('Optional components cannot be hidden dependencies: '+id);
    }
  }
  closure([...index.keys()],index);
  return index;
}
function closure(ids,index) {
  const done=new Set(),active=new Set(),ordered=[];
  function visit(id) {
    if (active.has(id)) fail('Dependency cycle: '+id);
    if (done.has(id)) return;
    const item=index.get(id);if(!item)fail('Unknown component: '+id);
    active.add(id);item.requires.forEach(visit);active.delete(id);done.add(id);ordered.push(id);
  }
  ids.forEach(visit);return ordered;
}
function requestedIds(selected,index) {
  if(!Array.isArray(selected))fail('Selected components must be an array.');
  for(const id of selected)if(!index.has(id)||index.get(id).kind!=='component')fail('Unknown optional component: '+String(id));
  const chosen=new Set(selected);return [...index.keys()].filter(id=>chosen.has(id));
}
function inventoryIndex(records,index) {
  if(!Array.isArray(records))fail('Inventory must be an array of verified receipts.');
  const result=new Map();
  for(const record of records) {
    if(!record||!index.has(record.id)||result.has(record.id))fail('Invalid or duplicate inventory component.');
    if(record.verified!==true||!validVersion(record.version)||!validHash(record.sha256)||!PLATFORMS.includes(record.platform))fail('A verified, platform-specific inventory receipt is required.');
    result.set(record.id,{...record,sha256:record.sha256.toLowerCase()});
  }
  return result;
}
function trustedOriginSet(origins) {
  if(!Array.isArray(origins))fail('Trusted origins must be an explicit array.');
  return new Set(origins.map(origin=>{
    let parsed;try{parsed=new URL(origin);}catch{fail('Invalid trusted distribution origin.');}
    if(parsed.protocol!=='https:'||parsed.origin!==origin||parsed.username||parsed.password)fail('Trusted distribution origins must be exact HTTPS origins.');
    return origin;
  }));
}
export function validateArtifact(artifact,{id,platform,trustedOrigins=[]}) {
  const origins=trustedOrigins instanceof Set?trustedOrigins:trustedOriginSet(trustedOrigins);
  if(!artifact||artifact.componentId!==id||artifact.platform!==platform)fail('Artifact component or platform mismatch.');
  if(!validVersion(artifact.version)||!validHash(artifact.sha256))fail('Artifact requires a valid version and SHA-256 digest.');
  if(!Number.isSafeInteger(artifact.sizeBytes)||artifact.sizeBytes<=0)fail('Artifact requires a positive, safe byte count.');
  const name=artifact.fileName;
  if(typeof name!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(name)||name.endsWith('.')||/^(con|prn|aux|nul|com[0-9]|lpt[0-9])(?:\.|$)/i.test(name))fail('Unsafe artifact filename.');
  if(typeof artifact.url!=='string'||artifact.url.length>4096)fail('Invalid artifact URL.');
  let url;try{url=new URL(artifact.url);}catch{fail('Invalid artifact URL.');}
  const host=url.hostname;
  if(url.protocol!=='https:'||url.username||url.password||url.hash||!origins.has(url.origin))fail('Artifact URL must use an explicitly trusted HTTPS origin without credentials or fragments.');
  if(!host.includes('.')||/^[0-9.]+$/.test(host)||host.includes(':')||host.endsWith('.')||/(^|\.)(localhost|local|internal)$/.test(host))fail('Artifact URL must use a public distribution hostname.');
  return Object.freeze({componentId:id,platform,version:artifact.version,sha256:artifact.sha256.toLowerCase(),sizeBytes:artifact.sizeBytes,fileName:name,url:url.href});
}
const matches=(receipt,artifact)=>receipt&&receipt.platform===artifact.platform&&receipt.version===artifact.version&&receipt.sha256===artifact.sha256;
/** Pure planning only. Receipts come from a native installer, never preference storage. */
export function planInstallation({selected=[],platform=null,catalog=CATALOG,installed=[],downloaded=[],trustedOrigins=[]}={}) {
  if(platform!==null&&!PLATFORMS.includes(platform))fail('Unsupported target platform.');
  const index=indexCatalog(catalog),requested=requestedIds(selected,index),required=closure(requested,index);
  const installedIndex=inventoryIndex(installed,index),downloadedIndex=inventoryIndex(downloaded,index),origins=trustedOriginSet(trustedOrigins);
  const roots=new Map(requested.map(id=>[id,new Set(closure([id],index))])),byId=new Map();
  const operations=required.map(id=>{
    const item=index.get(id),existing=installedIndex.get(id);
    const base={id,title:item.title,kind:item.kind,requested:requested.includes(id),requiredBy:requested.filter(root=>root!==id&&roots.get(root).has(id))};
    let operation;
    if(!platform)operation={...base,status:'blocked',code:'platform-required',message:'Choose the platform for this installation.'};
    else {
      const candidate=item.distribution?.[platform];
      if(!candidate)operation=existing?.platform===platform
        ?{...base,status:'installed',action:'keep',version:existing.version}
        :{...base,status:'blocked',code:item.distribution==null?'installer-unavailable':'platform-unavailable',message:item.distribution==null?(item.unavailableReason||'A signed release has not been configured.'):'No release is available for this platform.'};
      else {
        const artifact=validateArtifact(candidate,{id,platform,trustedOrigins:origins});
        operation=matches(existing,artifact)?{...base,status:'installed',action:'keep',version:existing.version}
          :{...base,status:'ready',action:existing?.platform===platform?'update':'install',artifact,downloadNeeded:!matches(downloadedIndex.get(id),artifact)};
      }
      if(operation.status!=='blocked'&&item.requires.some(dependency=>byId.get(dependency)?.status==='blocked'))operation={...base,status:'blocked',code:'dependency-unavailable',message:'A required runtime is not available for installation.'};
    }
    byId.set(id,operation);return operation;
  });
  const blockers=operations.filter(item=>item.status==='blocked').map(({id,code,message})=>({id,code,message}));
  const ready=operations.filter(item=>item.status==='ready'),canApply=blockers.length===0&&ready.length>0;
  const installs=canApply?ready.map(({id,action,artifact})=>({id,action,artifact})):[];
  const downloads=canApply?ready.filter(item=>item.downloadNeeded).map(item=>item.artifact):[];
  const downloadBytes=downloads.reduce((sum,item)=>sum+item.sizeBytes,0);
  if(!Number.isSafeInteger(downloadBytes))fail('Download byte total exceeds the safe integer limit.');
  return {schemaVersion:1,requested,required,operations,blockers,canApply,downloads,installs,downloadBytes,removals:[]};
}
/** A separate, explicit removal request. User data is always preserved. */
export function planRemoval({componentId,platform,installed=[],catalog=CATALOG}) {
  if(!PLATFORMS.includes(platform))fail('Unsupported target platform.');
  const index=indexCatalog(catalog);requestedIds([componentId],index);
  const inventory=inventoryIndex(installed,index),isPresent=id=>inventory.get(id)?.platform===platform;
  if(!isPresent(componentId))return {componentId,operations:[],preserveUserData:true};
  const remainingRoots=[...inventory.keys()].filter(id=>id!==componentId&&isPresent(id)&&index.get(id).kind==='component');
  const keep=new Set(closure(remainingRoots,index));
  const operations=closure([componentId],index).reverse().filter(id=>isPresent(id)&&!keep.has(id)).map(id=>({id,action:'remove',preserveUserData:true}));
  return {componentId,operations,preserveUserData:true};
}
