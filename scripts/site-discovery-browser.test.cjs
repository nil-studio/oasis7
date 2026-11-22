const {chromium}=require(process.env.OASIS7_PLAYWRIGHT_MODULE || 'playwright');
const assert=require('node:assert/strict');const fs=require('node:fs');const path=require('node:path');
const origin=process.env.OASIS7_SITE_QA_URL || 'http://127.0.0.1:48760';
(async()=>{
 const browser=await chromium.launch({headless:process.env.OASIS7_SITE_QA_HEADLESS === '1', ...(process.env.OASIS7_SITE_QA_BROWSER ? {executablePath:process.env.OASIS7_SITE_QA_BROWSER} : {})});
 const out=path.join(process.cwd(),'output/site-discovery-qa');const results=[];
 const record=(name)=>{results.push({name,result:'pass'});console.log('PASS '+name);};
 const context=async(opts={})=>browser.newContext({locale:'zh-CN',reducedMotion:'reduce',...opts});
 try{
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale,javaScriptEnabled:false,viewport:{width:360,height:800}});const p=await c.newPage();await p.goto(origin+(locale==='zh-CN'?'/':'/en/'));
  assert.equal(await p.locator('[data-decision-panel]:visible').count(),4);assert.equal(await p.locator('.topbar .nav a:visible').count(),4);
  await p.locator('[data-preview-details] > summary').click();
  assert.equal(await p.locator('noscript a:visible').count(),3);assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth),360);
  await p.screenshot({path:path.join(out,locale+'-nojs-360.png'),fullPage:true});await c.close();record(locale+' no-JS complete steps/nav/manual packages');
 }
 // Keep the integrated hero, while allowing readable text and evidence to lengthen the page.
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale});await c.route('https://api.github.com/**',r=>r.abort());const p=await c.newPage();
  for(const width of [360,390,768,1024,1440]){
   await p.setViewportSize({width,height:width===360?800:width===390?844:900});await p.goto(origin+(locale==='zh-CN'?'/':'/en/'));
   const layout=await p.evaluate(()=>({proof:document.querySelector('#proof').getBoundingClientRect().top,
    fourth:document.querySelector('[data-decision-step="3"]').getBoundingClientRect().bottom,
    art:getComputedStyle(document.querySelector('.hero-visual')).position,
    rail:getComputedStyle(document.querySelector('.decision-panels')).gridTemplateColumns.split(' ').length}));
   assert.equal(await p.locator('.discovery-editorial-grid a[href*="story"]').count(),0);assert.equal(await p.locator('main > #world-laws').count(),1);assert.equal(await p.locator('#contribute #world-laws').count(),0);
   assert.equal(layout.art,'absolute');
   assert.equal(layout.rail,width<=390?1:width<1100?2:4);
   assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth),width);
   assert.equal(await p.locator('#proof > .decision-walkthrough > h2').count(),1);
   assert.equal(await p.locator('.decision-panels h3').count(),4);
   assert.equal(await p.locator('.controlled-case').count(),0);
   assert.equal(await p.locator('.historical-viewer').getAttribute('open'),null);
   assert.equal(await p.locator('.viewer-evidence img').isVisible(),false);
   assert.deepEqual(await p.locator('.topbar .nav a').evaluateAll(a=>a.map(e=>e.getAttribute('href'))),['#proof','#progress','#lore','#download']);
   assert.equal(await p.locator('[data-decision-step]').count(),4);
   assert.equal(await p.locator('button[data-decision-step]').count(),0);
   assert.equal(await p.locator('.editorial-synopsis:visible').count(),2);
   assert.equal(await p.locator('.discovery-editorial-grid .entry-art:visible').count(),0);
   assert.equal(await p.locator('.case-art img:visible').count(),1);
   assert.equal(await p.locator('.preview-faq details').count(),3);
   assert.equal(await p.locator('#download > .preview-quickfacts').isVisible(),true);
   assert.equal(await p.locator('#lore > h2').isVisible(),true);
   const order=await p.locator('#proof, #progress, #download, .preview-faq, #lore').evaluateAll(nodes=>nodes.map(n=>n.id||n.className.split(' ')[0]));
   assert.deepEqual(order,['proof','progress','download','preview-faq','lore']);
   await p.screenshot({path:path.join(out,locale+'-'+width+'-observatory.png'),fullPage:true});

   assert.equal(await p.locator('.discovery-editorial-grid #world').count(),0);
   if(width<=390){
    const menu=await p.locator('[data-menu-toggle]').boundingBox();const language=await p.locator('.lang-switch').boundingBox();
    assert.ok(menu.x+menu.width<=language.x,JSON.stringify({menu,language}));
   }
   if(width===390){
    const sizes=await p.evaluate(()=>Object.fromEntries(['.decision-panels article > p:not(.decision-number)','.preview-badge','.hero-entry-note','.hero-telemetry-label'].map(s=>[s,parseFloat(getComputedStyle(document.querySelector(s)).fontSize)])));
    assert.ok(sizes['.decision-panels article > p:not(.decision-number)']>=15,JSON.stringify(sizes));
    assert.ok(sizes['.preview-badge']>=13 && sizes['.hero-entry-note']>=13 && sizes['.hero-telemetry-label']>=12,JSON.stringify(sizes));
    const caption=await p.evaluate(()=>({top:document.querySelector('.hero-telemetry-label').getBoundingClientRect().top,noteBottom:document.querySelector('.hero-entry-note').getBoundingClientRect().bottom}));
    assert.ok(caption.top>=caption.noteBottom,JSON.stringify(caption));
   }
  }
  await c.close();record(locale+' integrated hero, readable static steps and direct short summaries');
 }
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale});const p=await c.newPage();
  for(const width of [360,390,768,1024,1440]){
   await p.setViewportSize({width,height:width===360?800:width===390?844:900});await p.goto(origin+(locale==='zh-CN'?'/':'/en/')+'devlog/first-iron-batch/');await p.evaluate(()=>document.fonts.ready);
   const box=await p.evaluate(()=>({title:document.querySelector('h1').getBoundingClientRect().top,header:document.querySelector('.topbar').getBoundingClientRect().bottom,width:innerWidth,scroll:document.documentElement.scrollWidth}));
   assert.ok(box.title>=box.header,JSON.stringify(box));assert.equal(box.scroll,box.width);
   assert.equal(await p.locator('meta[property="og:type"]').getAttribute('content'),'article');
   assert.equal(await p.locator(".content-meta time:visible").getAttribute("datetime"),"2026-10-15");
   const cover=await p.locator('meta[property="og:image"]').getAttribute('content');assert.ok(cover.includes('first-iron-batch-'));
   assert.equal(await p.locator('.case-verification').getAttribute('open'),null);
  }
  await c.close();record(locale+' article header stays in flow without covering title');
 }
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale});const p=await c.newPage();await p.goto(origin+(locale==='zh-CN'?'/':'/en/'));
  await p.locator('.topbar .nav a[href="#lore"]').click();assert.equal(await p.locator('#lore > details').evaluate(e=>e.open),true);
  const href=await p.locator('#lore a[href*="story/"]').getAttribute('href');assert.ok(href.includes('story/'));
  assert.equal(await p.locator('#progress a[href="feed.xml"]').count(),1);
  await c.close();record(locale+' world lore opens its clearly labeled introduction and RSS is nearby');
 }
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale});const p=await c.newPage();const route=locale==='zh-CN'?'/':'/en/';await p.goto(origin+route);
  await p.locator('.portal-rules a[href="#world-laws"]').click();
  assert.equal(await p.locator('#world-laws > details').getAttribute('open'),'');
  await p.locator('#world-laws > details > summary').click();await p.waitForTimeout(100);
  assert.equal(await p.locator('#world-laws > details').getAttribute('open'),null);
  await p.locator('.portal-rules a[href="#world-laws"]').click();
  assert.equal(await p.locator('#world-laws > details').getAttribute('open'),'');
  const deep=await c.newPage();await deep.goto(origin+route+'#world-laws');
  assert.equal(await deep.locator('#world-laws > details').getAttribute('open'),'');
  await c.close();record(locale+' world rules CTA and initial deep link reveal actual body');
 }
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale});await c.route('https://api.github.com/**',r=>r.abort());const p=await c.newPage();
  const route=locale==='zh-CN'?'/':'/en/';await p.goto(origin+route);
  assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),null);
  await p.locator('.hero-overview-link').click();assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),'');
  assert.equal(await p.locator('[data-download-primary-trust]').isVisible(),true);
  await p.locator('[data-preview-details] > summary').focus();await p.keyboard.press('Enter');assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),null);
  await p.goto(origin+route);
  await p.evaluate(()=>{document.querySelector('.hero-overview-link').click();document.querySelector('[data-preview-details] > summary').click();});
  await p.waitForTimeout(100);assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),null);
  await p.locator('.hero-overview-link').click();assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),'');
  const deep=await c.newPage();await deep.goto(origin+route+'#download');assert.equal(await deep.locator('[data-preview-details]').getAttribute('open'),'');
  await p.goto(origin+route);await p.evaluate(()=>{window.location.hash='#download';});
  await p.waitForFunction(()=>document.querySelector('[data-preview-details]').open);
  assert.equal(await p.locator('[data-preview-details]').getAttribute('open'),'');
  await c.close();record(locale+' compact preview opens by CTA, keyboard-native disclosure and deep link with visible risk');
 }
 // Deep links must land in the viewport, not merely open the disclosure.
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale,viewport:{width:390,height:844}});const p=await c.newPage();
  const route=locale==='zh-CN'?'/':'/en/';
  await p.goto(origin+route+'#package-download');
  assert.equal(await p.locator('[data-preview-details]').evaluate(e=>e.open),true);
  await p.waitForFunction(()=>{const r=document.querySelector('#package-download').getBoundingClientRect();return r.top>=0&&r.top<innerHeight/2;});
  await p.locator('[data-preview-details] > summary').click();
  await p.locator('.preview-paths a[href="#package-download"]').click();
  assert.equal(await p.locator('[data-preview-details]').evaluate(e=>e.open),true);
  await p.waitForFunction(()=>{const r=document.querySelector('#package-download').getBoundingClientRect();return r.top>=0&&r.top<innerHeight/2;});
  await c.close();record(locale+' platform deep link and repeated link reveal and position the destination');
 }
 // Concept media failure must not remove the decision path or disclosure.
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale,viewport:{width:390,height:844},reducedMotion:'no-preference'});
  await c.route('https://api.github.com/**',r=>r.abort());
  await c.route('**/assets/images/hero/*.webp',r=>r.abort());
  const p=await c.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));
  await p.goto(origin+(locale==='zh-CN'?'/':'/en/'));
  assert.equal(await p.locator('.hero .button.primary').isVisible(),true);
  assert.equal(await p.locator('.hero-telemetry-label').isVisible(),true);
  assert.equal(await p.locator('[data-decision-panel]:visible').count(),4);
  await p.emulateMedia({reducedMotion:'reduce'});
  assert.equal(await p.locator('.hero .button.primary').evaluate(e=>getComputedStyle(e).transitionDuration),'0s');
  await p.locator('.preview-faq summary').first().click();
  assert.equal(await p.locator('.preview-faq details').first().getAttribute('open'),'');
  assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth),390);
  assert.deepEqual(errors,[]);
  await p.screenshot({path:path.join(out,locale+'-media-failure-390.png'),fullPage:true});
  await c.close();record(locale+' failed concept media and runtime reduced-motion retain readable controls and FAQ');
 }
 const files=['oasis7-windows-x64.exe','oasis7-macos-x64.dmg','oasis7-linux-x64.deb','oasis7-checksums.txt'];
 for(const locale of ['zh-CN','en-US']) for(const scenario of ['full','no-platform','no-checksum','invalid','timeout']){
  const c=await context({locale});let requestUrl;await c.route('https://api.github.com/**',async r=>{
   requestUrl=r.request().url();
   if(scenario==='timeout')return r.abort();
   const names=files.filter(n=>!(scenario==='no-platform'&&n===files[0])&&!(scenario==='no-checksum'&&n===files[3]));
   await r.fulfill({json:{tag_name:'qa-candidate',published_at:scenario==='invalid'?'bad':'2026-10-14T00:00:00Z',assets:names.map(name=>({name,browser_download_url:`https://github.com/nil-studio/oasis7/releases/download/qa-candidate/${name}`}))}});
  });const p=await c.newPage();await p.goto(origin+(locale==='zh-CN'?'/':'/en/'));await p.locator('[data-preview-details] > summary').click();await p.locator('[data-download-platform-button="windows"]').click();
  await p.waitForTimeout(150);
  assert.equal(requestUrl,'https://api.github.com/repos/nil-studio/oasis7/releases/latest');
  const tag=await p.locator('[data-release-tag]').innerText();const href=await p.locator('[data-download-primary-link]').getAttribute('href');
  if(scenario==='invalid'||scenario==='timeout'){assert.equal(tag,'2026-06-03_v2');assert.equal(href,'https://github.com/nil-studio/oasis7/releases/download/2026-06-03_v2/'+files[0]);}
  else {assert.equal(tag,'qa-candidate');if(scenario==='full')assert.equal(href,'https://github.com/nil-studio/oasis7/releases/download/qa-candidate/'+files[0]);else assert.equal(href,null);}
  await c.close();record(locale+' release '+scenario+' coherent metadata/assets or safe fallback');
 }
 for (const userAgent of ['Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile', 'Mozilla/5.0 (Windows NT 10.0; ARM64)', 'Mozilla/5.0 (X11; Linux aarch64)']) {
  const c=await context({userAgent});await c.route('https://api.github.com/**',r=>r.abort());const p=await c.newPage();await p.goto(origin+'/');assert.equal(await p.locator('[data-download-primary-link]').getAttribute('href'),null);assert.equal(await p.locator('[data-download-platform-button][aria-pressed="true"]').count(),0);await c.close();record('unsupported device has no recommendation: '+userAgent);
 }
 {
  const c=await context({viewport:{width:390,height:844}});await c.route('https://api.github.com/**',r=>r.abort());const p=await c.newPage();await p.goto(origin+'/');
  assert.equal(await p.locator('[data-decision-panel]:visible').count(),4);
  assert.equal(await p.locator('[data-decision-step][tabindex]').count(),0);
  await p.locator('[data-menu-toggle]').click();assert.equal(await p.locator('.topbar .nav a:visible').count(),4);await p.keyboard.press('Escape');assert.equal(await p.locator('[data-menu-toggle]').getAttribute('aria-expanded'),'false');
  await p.locator('#proof').scrollIntoViewIfNeeded();await p.screenshot({path:path.join(out,'keyboard-proof-390.png')});
  await p.evaluate(()=>document.documentElement.style.fontSize='200%');assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth),390);
  const canvas=await p.locator('[data-hero-canvas]').count();assert.equal(canvas,0);
  await c.close();record('static steps, keyboard menu/Escape and 200 percent text zoom');
 }
 {
  const c=await context({reducedMotion:'no-preference'});await c.addInitScript(()=>{window.heroCanvasContexts=0;const original=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(...args){if(this.matches('[data-hero-canvas]'))window.heroCanvasContexts++;return original.apply(this,args);};});
  const p=await c.newPage();await p.goto(origin+'/');await p.waitForTimeout(100);
  assert.equal(await p.evaluate(()=>window.heroCanvasContexts),0);assert.equal(await p.locator('canvas[data-hero-canvas]').count(),0);
  await c.close();record('normal-motion homepage creates no hidden Canvas context');
 }
 for(const locale of ['zh-CN','en-US']){
  const c=await context({locale,viewport:{width:390,height:844}});await c.route('https://api.github.com/**',r=>r.abort());const p=await c.newPage();const route=locale==='zh-CN'?'/':'/en/';await p.goto(origin+route);
  assert.equal(await p.locator('[data-case-result]').count(),1);assert.equal(await p.locator('section#world').count(),0);assert.equal(await p.locator('footer #world').getAttribute('href'),'observations/');
  await p.locator('.case-source a[href="#case-conditions"]').click();assert.equal(await p.locator('#case-conditions').getAttribute('open'),'');
  await p.locator('#case-conditions > summary').click();await p.locator('.case-source a[href="#case-conditions"]').click();assert.equal(await p.locator('#case-conditions').getAttribute('open'),'');
  await p.locator('.hero-overview-link').click();await p.locator('.preview-paths a[href="#package-download"]').click();await p.waitForTimeout(700);
  assert.ok(await p.locator('[data-download-platform-button="windows"]').isVisible());const top=await p.locator('#package-download').evaluate(e=>e.getBoundingClientRect().top);assert.ok(top>=-1&&top<250,String(top));
  assert.equal(await p.locator('meta[name="oasis7-site-build"]').getAttribute('content'),'20261016-observatory');await c.close();record(locale+' result, separate setup paths and direct mobile platform access');
 }
 {
  const c=await context({viewport:{width:390,height:844}});const p=await c.newPage();await p.goto(origin+'/story/');await p.waitForFunction(()=>document.querySelector('[data-story-reader]').dataset.readerState==='ready');
  assert.equal(await p.locator('.reader-nav-details').getAttribute('open'),null);await p.locator('.reader-nav-details > summary').click();await p.locator('[data-story-src$="chapter-02.md"]').first().click();await p.waitForFunction(()=>document.querySelector('[data-story-reader]').dataset.readerState==='ready');
  await p.locator('[data-story-start]').click();await p.waitForFunction(()=>document.querySelector('[data-story-reader]').dataset.readerState==='ready');assert.ok((await p.locator('[data-story-src][aria-pressed="true"]').getAttribute('data-story-src')).endsWith('volume-01-chapter-01.md'));
  assert.equal(await p.title(),'绿洲2076｜oasis7世界观小说');await c.close();record('novel start returns to first chapter after a different selection');
 }
 // Reader empty/error/retry and both late response directions.
 for(const first of ['empty','error','late-success','late-error']){
  const c=await context({locale:'en-US'});let attempts=0;
  await c.route('**/volume-01-chapter-01.md',async r=>{
   attempts++;
   if(first.startsWith('late-'))await new Promise(ok=>setTimeout(ok,700));
   if((first==='error'&&attempts===1)||first==='late-error')await r.fulfill({status:404,body:'not found'});
   else await r.fulfill({body:first==='empty'&&attempts===1?'   ':'# Current chapter\n\nReadable current text.'});
  });await c.route('**/volume-01-chapter-02.md',r=>r.fulfill({body:'# New chapter\n\nB is authoritative for this reading selection.'}));
  const p=await c.newPage();await p.goto(origin+'/story/');
  assert.ok(p.url().endsWith('/story/'));
  if(first.startsWith('late-')){await p.locator('.reader-nav-details > summary').click();await p.locator('[data-story-src$="chapter-02.md"]').first().click();await p.waitForTimeout(950);assert.ok((await p.locator('[data-story-content]').innerText()).includes('B is authoritative'));assert.equal(await p.locator('.reader-panel').getAttribute('aria-busy'),'false');}
  else {await p.waitForFunction(()=>document.querySelector('[data-story-reader]').getAttribute('data-reader-state')==='empty'||document.querySelector('[data-story-reader]').getAttribute('data-reader-state')==='error');assert.equal(await p.locator('[data-story-content]').innerText(),'');await p.locator('[data-story-retry]').click();await p.waitForFunction(()=>document.querySelector('[data-story-reader]').getAttribute('data-reader-state')==='ready');assert.ok((await p.locator('[data-story-content]').innerText()).includes('Readable current text'));}
  await c.close();record('reader '+first+' / real recovery / English does not invent route');
 }
 {
  const c=await context({locale:'en-US'});await c.route('https://api.github.com/**',r=>r.abort());
  await c.route(origin+'/project/**',async r=>{const response=await r.fetch({url:r.request().url().replace('/project/','/')});await r.fulfill({response});});
  const p=await c.newPage();await p.goto(origin+'/project/devlog/discovery-first/?from=qa#top');await p.waitForURL('**/project/en/devlog/discovery-first/index.html?from=qa#top');
  assert.ok(p.url().includes('/project/en/devlog/discovery-first/index.html?from=qa#top'));await c.close();record('explicit locale mapping preserves project prefix query and hash');
 }
 {
  const c=await context();let attempts=0;
  await c.route('**/volume-01-chapter-01.md',async r=>{attempts++;const first=attempts===1;await new Promise(ok=>setTimeout(ok,first?600:900));await r.fulfill({body:first?'   ':'# Fresh chapter\n\nFresh after retry.'});});
  const p=await c.newPage();await p.goto(origin+'/story/');await p.locator('[data-story-retry]').click();await p.waitForTimeout(1100);
  assert.ok((await p.locator('[data-story-content]').innerText()).includes('Fresh after retry'));
  await p.locator('.reader-nav-details > summary').click();await p.locator('[data-story-src$="chapter-02.md"]').first().click();await p.waitForFunction(()=>document.querySelector('[data-story-reader]').getAttribute('data-reader-state')==='ready');
  await p.locator('[data-story-src$="chapter-01.md"]').first().click();await p.waitForFunction(()=>document.querySelector('[data-story-reader]').getAttribute('data-reader-state')==='ready');
  assert.ok((await p.locator('[data-story-content]').innerText()).includes('Fresh after retry'));await c.close();record('same chapter retry ignores late cache writer');
 }
 fs.writeFileSync(path.join(out,'behavior-results.json'),JSON.stringify(results,null,2));
 }finally{await browser.close();}
})();
