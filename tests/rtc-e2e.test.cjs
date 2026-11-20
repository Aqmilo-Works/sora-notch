// Send > QR Code over WebRTC, end to end: the Rust side (rtc_share.rs, run as an ignored test) and the phone page
// (share-service/public/s) in a real browser. NODE_PATH=../test-env/node_modules node tests/rtc-e2e.test.cjs
const assert=require('assert'),http=require('http'),fs=require('fs'),path=require('path'),{spawn}=require('child_process'),{chromium}=require('playwright');
const root=path.join(__dirname,'..','share-service','public');
const CSP="default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
const server=http.createServer((req,res)=>{let p=decodeURIComponent(new URL(req.url,'http://x').pathname);if(p.endsWith('/'))p+='index.html';const f=path.join(root,p);if(!f.startsWith(root)||!fs.existsSync(f)){res.statusCode=404;return res.end()}
  res.setHeader('Content-Security-Policy',CSP);res.setHeader('Content-Type',{'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml'}[path.extname(f)]||'application/octet-stream');res.end(fs.readFileSync(f))});
(async()=>{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));const base='http://127.0.0.1:'+server.address().port+'/s/';
  const rust=spawn('cargo',['test','--release','--quiet','serves_a_browser','--','--ignored','--nocapture'],{cwd:path.join(__dirname,'..'),env:{...process.env,E2E_SECONDS:'120'}});
  const lines=[];let buf='';rust.stdout.on('data',d=>{buf+=d;let i;while((i=buf.indexOf('\n'))>=0){lines.push(buf.slice(0,i).trim());buf=buf.slice(i+1)}});rust.stderr.on('data',()=>{});
  const until=async(test,ms=60000)=>{const t=Date.now();while(Date.now()-t<ms){const l=lines.find(test);if(l)return l;await new Promise(r=>setTimeout(r,100))}throw new Error('waited for a line; got:\n'+lines.join('\n'))};
  const latest=()=>{const l=[...lines].reverse().find(x=>x.startsWith('LINK '));const [,url,code]=l.split(' ');return {url:url.replace('PAGE/',base),code}};
  let b;
  try{
    await until(l=>l.startsWith('LINK '),300000);const inbox=(await until(l=>l.startsWith('INBOX '))).slice(6);
    b=await chromium.launch({channel:process.env.PW_CHANNEL||'chrome'});const ctx=await b.newContext({acceptDownloads:true});
    // 1. Download: wrong code first, then the right one, then the file; the link is used up after that.
    const first=latest();let page=await ctx.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));page.on('console',m=>{if(m.type()==='error')errors.push(m.text())});
    await page.goto(first.url);await page.waitForSelector('#unlock:not([hidden])',{timeout:20000});
    assert.equal(await page.evaluate(()=>location.hash),'','the secret leaves the address bar');
    assert.match(await page.textContent('#shown-on'),/TEST-PC/);
    await page.fill('#code','000000');await page.waitForFunction(()=>document.querySelector('#code-err').textContent.includes('not right'));
    await page.fill('#code',latest().code);await page.waitForSelector('#list:not([hidden])');
    assert.equal(await page.textContent('#list-title'),'1 file');assert.match(await page.textContent('#who-name'),/./);
    await until(l=>l.startsWith('EVENT connected'));
    const td=Date.now();const [dl]=await Promise.all([page.waitForEvent('download',{timeout:120000}),page.click('#rows .file')]);
    assert.equal(dl.suggestedFilename(),'hello.txt');const got=fs.readFileSync(await dl.path());
    if(process.env.E2E_DOWN_MB){assert.equal(got.length,process.env.E2E_DOWN_MB*1048576);console.log('download took',Date.now()-td,'ms for',got.length,'bytes')}else assert.equal(got.toString(),'hello from the PC');
    await until(l=>l.startsWith('EVENT downloaded'));await page.waitForFunction(()=>document.querySelector('#wait-title').textContent==='All done');
    // 2. The old link is burned; the new one (new code) takes an upload, approved on the PC.
    const old=await ctx.newPage();await old.goto(first.url);await old.waitForFunction(()=>document.querySelector('#wait-title').textContent==='All done');
    const second=latest();assert.notEqual(second.url,first.url,'a new link after use');
    page=await ctx.newPage();page.on('pageerror',e=>errors.push(String(e)));await page.goto(second.url);await page.waitForSelector('#unlock:not([hidden])',{timeout:20000});
    await page.fill('#code',latest().code);await page.waitForSelector('#list:not([hidden])');
    assert.equal(await page.textContent('#list-title'),'1 file','the files stay for the next link');
    const big=Buffer.alloc((+process.env.E2E_MB||3)*1024*1024+123,7);const t0=Date.now();
    await page.setInputFiles('#pick',process.env.E2E_FILE?[process.env.E2E_FILE]:[{name:'photo.jpg',mimeType:'image/jpeg',buffer:big},{name:'note.txt',mimeType:'text/plain',buffer:Buffer.from('hi')}]);
    await until(l=>l.startsWith('EVENT offer'));await page.waitForFunction(()=>document.querySelector('#wait-title').textContent==='Sent'||document.querySelector('#xinfo').textContent==='Sent to the PC',{timeout:60000});
    fs.writeFileSync(path.join(inbox,'stop.txt'),'');await until(l=>l==='DONE');console.log('upload took',Date.now()-t0,'ms for',process.env.E2E_FILE?fs.statSync(process.env.E2E_FILE).size:big.length+2,'bytes');
    if(!process.env.E2E_FILE)assert(lines.includes('GOT photo.jpg '+big.length),'the photo arrived whole: '+lines.filter(l=>l.startsWith('GOT')).join(', '));if(!process.env.E2E_FILE)assert(lines.includes('GOT note.txt 2'));
    assert.deepEqual(errors,[],'no page errors: '+errors.join(' / '));
    console.log('PASS WebRTC: code (wrong, right), download, used link, new link, approved upload of 2 files (3 MB)');
  }finally{if(b)await b.close();rust.kill();server.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
