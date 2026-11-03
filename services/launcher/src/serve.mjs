import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const routes=new Map([
 ['/', ['../web/index.html','text/html; charset=utf-8']],
 ['/app.mjs',['../web/app.mjs','text/javascript; charset=utf-8']],
 ['/styles.css',['../web/styles.css','text/css; charset=utf-8']],
 ['/catalog.mjs',['./catalog.mjs','text/javascript; charset=utf-8']],
 ['/planner.mjs',['./planner.mjs','text/javascript; charset=utf-8']],
]);
export function createLauncherServer() {
 return http.createServer(async(request,response)=>{
  response.setHeader('Cache-Control','no-store');
  response.setHeader('X-Content-Type-Options','nosniff');
  response.setHeader('Referrer-Policy','no-referrer');
  response.setHeader('Content-Security-Policy',"default-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'");
  if(!['GET','HEAD'].includes(request.method)){response.writeHead(405,{Allow:'GET, HEAD'}).end('Method not allowed');return;}
  let pathname;try{pathname=new URL(request.url,'http://localhost').pathname;}catch{response.writeHead(400).end('Bad request');return;}
  const route=routes.get(pathname);if(!route){response.writeHead(404).end('Not found');return;}
  try{const data=await readFile(new URL(route[0],import.meta.url));response.writeHead(200,{'Content-Type':route[1]});response.end(request.method==='HEAD'?undefined:data);}
  catch{response.writeHead(500).end('Unable to load launcher');}
 });
}
if(process.argv[1]===fileURLToPath(import.meta.url)){
 const port=Number(process.env.METALLIC_LAUNCHER_PORT||4173);
 if(!Number.isInteger(port)||port<1||port>65535)throw new Error('Invalid METALLIC_LAUNCHER_PORT');
 createLauncherServer().listen(port,'127.0.0.1',()=>console.log('Metallic component setup: http://127.0.0.1:'+port));
}
