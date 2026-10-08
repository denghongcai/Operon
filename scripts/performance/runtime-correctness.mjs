import assert from 'node:assert/strict';
import fs from 'node:fs';
import http2 from 'node:http2';
import { OperonClient } from '../../packages/sdk-js/dist/index.js';

const token = fs.readFileSync('/review/token','utf8').trim();
const client = new OperonClient([{nodeId:'local',endpoint:'grpc://daemon:7789',token}]);
const mib = 1024*1024;
const original = Buffer.from('ORIGINAL-CONTENT');
const data = Buffer.alloc(8*mib);
for(let i=0;i<data.length;i++) data[i]=i%251;
function variable(n) { const out=[]; do{out.push((n&127)|(n>127?128:0));n=Math.floor(n/128);}while(n);return Buffer.from(out); }
function bytes(field,value){return Buffer.concat([variable(field*8+2),variable(value.length),value]);}
async function sdkDeadlineAcceptance(){
 const server=http2.createServer();
 const sessions=new Set();
 let mode='headers';
 server.on('session',session=>{sessions.add(session);session.on('close',()=>sessions.delete(session));});
 server.on('stream',(stream,headers)=>{
  assert.equal(headers.authorization,'Bearer '+token);
  stream.on('error',()=>{}); // Client deadline cancellation is expected.
  if(mode==='headers')return;
  stream.respond({':status':200,'content-type':'application/grpc'},{waitForTrailers:true});
  if(mode==='body'){stream.write(Buffer.from([0,0,0,0,32]));return;}
  stream.on('wantTrailers',()=>stream.sendTrailers({'grpc-status':'0'}));
  stream.end(Buffer.alloc(5));
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const peer=new OperonClient([{nodeId:'fault',endpoint:`grpc://127.0.0.1:${server.address().port}`,token,
  transport:{connectTimeoutSecs:2,rpcTimeoutSecs:0.5,keepaliveIntervalSecs:0}}]);
 try{
  for(mode of ['headers','body'])await assert.rejects(peer.statFs('fault','/stall'),{code:4});
  mode='healthy';
  assert.equal((await peer.statFs('fault','/recovered')).size,0);
  console.log('PASS: real SDK header/body deadlines and recovery against deadline-ignoring HTTP/2 peer');
 }finally{
  peer.close();
  for(const session of sessions)session.destroy();
  await new Promise(resolve=>server.close(resolve));
 }
}
async function raw(method,messages){
 const session=http2.connect('http://daemon:7789');
 try{return await new Promise((resolve,reject)=>{
  const request=session.request({':method':'POST',':path':'/operon.runtime.v1.OperonRuntime/'+method,'content-type':'application/grpc',te:'trailers',authorization:'Bearer '+token});
  let status;
  request.on('response',h=>{if(h['grpc-status']!==undefined)status=Number(h['grpc-status']);});
  request.on('trailers',h=>status=Number(h['grpc-status']));
  request.on('data',()=>{});request.on('error',reject);request.on('end',()=>resolve(status));
  for(const body of messages){const header=Buffer.alloc(5);header.writeUInt32BE(body.length,1);request.write(Buffer.concat([header,body]));}
  request.end();
 });}finally{session.close();}
}
try{
 if(process.env.RESTART_CHECK){
  const record=JSON.parse(fs.readFileSync('/review/exec.json','utf8'));
  assert.equal((await client.getExec('local',record.id)).status,'succeeded');
  const logs=await client.listExecLogs('local',record.id);
  assert.ok(logs.logs.some(log=>Buffer.from(log.data).includes(Buffer.from('restart-marker'))));
  assert.ok((await client.listAudit('local')).events.length>0);
  console.log('PASS: exec, logs and audit restored after daemon restart');
 }else{
  await sdkDeadlineAcceptance();
  await client.writeFileBytes('local','/sdk-large',data);
  for(const size of [0,2*mib+1,4*mib+1,8*mib]){
   const read=await client.readFileRangeBytes('local','/sdk-large',0,size);
   assert.deepEqual(Buffer.from(read),data.subarray(0,size));
  }
  for(const offset of [37,data.length-7,data.length,data.length+99]){
   assert.deepEqual(Buffer.from(await client.readFileRangeBytes('local','/sdk-large',offset,4*mib)),data.subarray(offset,offset+4*mib));
  }
  assert.equal(await raw('WriteFileRange',[Buffer.concat([bytes(1,Buffer.from('/raw-large')),bytes(3,data)])]),0);
  assert.deepEqual(Buffer.from(await client.readFileRangeBytes('local','/raw-large',0,8*mib)),data);
  assert.equal(await raw('WriteFileRange',[Buffer.concat([bytes(1,Buffer.from('/raw-large')),bytes(3,Buffer.alloc(8*mib+1))])]),3);
  await client.writeFileBytes('local','/atomic',original);
  const target=bytes(1,bytes(1,Buffer.from('/atomic')));
  const chunk=bytes(2,bytes(1,Buffer.from('NEW')));
  assert.equal(await raw('WriteFile',[target,chunk,Buffer.alloc(0)]),3);
  assert.deepEqual(Buffer.from(await client.readFileBytes('local','/atomic')),original);
  // Concurrent reads append enough audit events to exercise shared store writes.
  await Promise.all(Array.from({length:8},async()=>{
   for(let i=0;i<40;i++) await client.readFileRangeBytes('local','/sdk-large',i,4096);
  }));
  const trace=await client.run({name:'restart-evidence',steps:[{id:'exec',node:'local',action:'exec.run',command:'printf restart-marker',cwd:'/'}]});
  assert.equal(trace.status,'succeeded',JSON.stringify(trace));
  const record=(await client.listExecs('local')).execs.find(record=>record.command==='printf restart-marker');
  assert.ok(record);fs.writeFileSync('/review/exec.json',JSON.stringify(record));
  console.log('PASS: SDK ranges, 8MiB writes, bounds, failed streaming replacement and concurrent audit');
 }
}finally{client.close();}
