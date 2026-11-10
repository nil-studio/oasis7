// Real-browser security regression; also runnable through the site QA contract.
const { chromium } = require(process.env.OASIS7_PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { checkEmbeddedProviders } = require('./site-deck-media-cases.cjs');

const site = path.resolve(__dirname, '../site');
const output = path.resolve(__dirname, '../output/site-deck-security-qa');
const types = {'.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
  '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.woff2': 'font/woff2'};

async function listen(handler) {
  const server = http.createServer(handler);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return server;
}

function origin(server) { return `http://127.0.0.1:${server.address().port}`; }
function close(server) { return new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); }

async function waitForDeckTransitions(page) {
  await page.waitForFunction(() => document.querySelector('.reveal')
    .getAnimations({subtree: true})
    .filter(animation => animation instanceof CSSTransition)
    .every(animation => animation.playState === 'finished' || animation.playState === 'idle'));
}

(async () => {
  let targetServer;
  let senderServer;
  let browser;
  const results = [];
  try {
    if (!process.env.OASIS7_SITE_QA_URL) {
      targetServer = await listen((request, response) => {
        try {
          const url = new URL(request.url, 'http://127.0.0.1');
          const pathname = decodeURIComponent(url.pathname);
          const file = path.resolve(site, '.' + pathname, pathname.endsWith('/') ? 'index.html' : '');
          assert.ok(file.startsWith(site + path.sep));
          const content = fs.readFileSync(file);
          response.writeHead(200, {'Content-Type': types[path.extname(file)] || 'application/octet-stream'});
          response.end(content);
        } catch {
          response.writeHead(404);
          response.end('not found');
        }
      });
    }
    const target = process.env.OASIS7_SITE_QA_URL || origin(targetServer);
    senderServer = await listen((_request, response) => {
      response.writeHead(200, {'Content-Type': 'text/html'});
      response.end(`<!doctype html><title>Deck cross-origin regression</title>
        <style>body{margin:0}iframe{border:0;width:100vw;height:100vh}</style>
        <iframe id="deck" title="Test deck"></iframe>
        <script>window.revealMessages=[];window.securityBarriers=0;addEventListener('message',event=>{
          if(event.source!==document.querySelector('#deck').contentWindow)return;
          try {const value=JSON.parse(event.data);
            if(value.namespace==='reveal')revealMessages.push(value);
            if(value.namespace==='oasis7-deck-test'&&value.event==='barrier')securityBarriers++;
          }catch{}
        });</script>`);
    });
    assert.notEqual(new URL(target).origin, origin(senderServer));
    browser = await chromium.launch({headless: process.env.OASIS7_SITE_QA_HEADLESS === '1',
      ...(process.env.OASIS7_SITE_QA_BROWSER ? {executablePath: process.env.OASIS7_SITE_QA_BROWSER} : {})});
    fs.mkdirSync(output, {recursive: true});
    for (const route of ['/deck/', '/deck/en/']) {
      const context = await browser.newContext({viewport: {width: 1440, height: 900}, reducedMotion: 'reduce'});
      const errors = [];
      context.on('page', page => page.on('pageerror', error => errors.push(error.message)));
      const parent = await context.newPage();
      await parent.goto(origin(senderServer));
      await parent.locator('#deck').evaluate((element, url) => { element.src = url; },
        target + route + '?postMessage=true&postMessageEvents=true');
      const frame = await (await parent.$('#deck')).contentFrame();
      await frame.waitForFunction(() => typeof deck !== 'undefined' && deck.isReady());
      await frame.evaluate(() => {
        window.receivedSecurityMessages = 0;
        addEventListener('message', () => { window.receivedSecurityMessages += 1; });
      });
      const initial = await frame.evaluate(() => deck.getState());
      const messages = [
        {method: 'slide', args: [3]},
        {method: 'configure', args: [{postMessage: true, postMessageEvents: true, controls: false}]},
        {method: 'slide', args: [4]},
        {method: 'destroy'},
        '{}', '{bad JSON}', 'unrelated message',
      ];
      for (const [index, message] of messages.entries()) {
        await parent.evaluate(value => {
          document.querySelector('#deck').contentWindow.postMessage(
            typeof value === 'string' ? value : JSON.stringify(value), '*');
        }, message);
        await frame.waitForFunction(count => window.receivedSecurityMessages >= count, index + 1);
        assert.deepEqual(await frame.evaluate(() => deck.getState()), initial, route + ': external API call changed state');
        assert.equal(await frame.locator('.reveal.ready').count(), 1, 'external destroy must not remove the deck');
        assert.equal(await frame.evaluate(() => deck.getConfig().controls), true);
      }
      // A local caller changing configuration cannot reinstall the removed capability.
      await frame.evaluate(() => deck.configure({postMessage: true, postMessageEvents: true}));
      await parent.evaluate(() => document.querySelector('#deck').contentWindow.postMessage(
        JSON.stringify({method: 'slide', args: [5]}), '*'));
      await frame.waitForFunction(() => window.receivedSecurityMessages === 8);
      assert.deepEqual(await frame.evaluate(() => deck.getState()), initial);
      // Trigger a normal local event after configure(true), then drain the same
      // sender's message queue before checking that no Reveal state was broadcast.
      await frame.evaluate(() => {
        deck.next();
        window.parent.postMessage(JSON.stringify({namespace: 'oasis7-deck-test', event: 'barrier'}), '*');
      });
      await frame.waitForFunction(() => deck.getIndices().h === 1);
      await parent.waitForFunction(() => window.securityBarriers === 1);
      assert.deepEqual(await parent.evaluate(() => window.revealMessages), []);

      const page = await context.newPage();
      await page.goto(target + route + '#/2');
      await page.waitForFunction(() => typeof deck !== 'undefined' && deck.isReady() && deck.getIndices().h === 2);
      await page.reload();
      await page.waitForFunction(() => typeof deck !== 'undefined' && deck.isReady() && deck.getIndices().h === 2);
      await page.keyboard.press('ArrowLeft');
      await page.waitForFunction(() => deck.getIndices().h === 1);
      await page.keyboard.press('ArrowRight');
      await page.waitForFunction(() => deck.getIndices().h === 2);
      await page.keyboard.press('ArrowRight');
      await page.waitForFunction(() => deck.getIndices().h === 3);
      await page.keyboard.press('ArrowRight');
      await page.waitForFunction(() => deck.getIndices().h === 3 && deck.getIndices().f === 0);
      await waitForDeckTransitions(page);
      await page.screenshot({path: path.join(output, route === '/deck/' ? 'zh-desktop.png' : 'en-desktop.png')});
      assert.deepEqual(errors, []);
      await context.close();

      const mobile = await browser.newContext({viewport: {width: 390, height: 844}, hasTouch: true,
        isMobile: true, reducedMotion: 'reduce'});
      const phone = await mobile.newPage();
      const mobileErrors = [];
      phone.on('pageerror', error => mobileErrors.push(error.message));
      await phone.goto(target + route);
      await phone.waitForFunction(() => typeof deck !== 'undefined' && deck.isReady());
      assert.equal(await phone.locator('[data-deck-count]').innerText(), '1 / 12');
      await phone.locator('[data-deck-next]').tap();
      await phone.waitForFunction(() => deck.getIndices().h === 1);
      assert.equal(await phone.locator('[data-deck-count]').innerText(), '2 / 12');
      await phone.locator('[data-deck-prev]').tap();
      await phone.waitForFunction(() => deck.getIndices().h === 0);
      assert.equal(await phone.locator('[data-deck-count]').innerText(), '1 / 12');
      await waitForDeckTransitions(phone);
      await phone.screenshot({path: path.join(output, route === '/deck/' ? 'zh-mobile.png' : 'en-mobile.png')});
      assert.deepEqual(mobileErrors, []);
      await mobile.close();
      results.push({route, result: 'pass', crossOriginMessages: messages.length + 1,
        hashReload: true, keyboardAndFragment: true, touchNavigation: true});
      console.log('PASS ' + route + ' message boundary, hash, keyboard, fragments and touch');
    }
    results.push(await checkEmbeddedProviders(browser, target));
    fs.writeFileSync(path.join(output, 'behavior-results.json'), JSON.stringify(results, null, 2) + '\n');
  } finally {
    if (browser) await browser.close();
    if (senderServer) await close(senderServer);
    if (targetServer) await close(targetServer);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
