// Exercise the shipped Reveal bundle with real cross-origin iframe documents.
const assert = require('node:assert/strict');

const frameDocument = `<!doctype html><title>Controlled media frame</title>
<script>addEventListener('message', event => {
  parent.postMessage({kind: 'media-case-message', message: event.data}, '*');
});</script>`;

function decodeMessage(value) {
  try { return JSON.parse(value); } catch { return value; }
}

async function drainMessages(page) {
  await page.evaluate(() => document.querySelector('#media-case').contentWindow
    .postMessage('__media_test_barrier__', '*'));
  await page.waitForFunction(() => window.mediaMessages.includes('__media_test_barrier__'));
  return page.evaluate(() => window.mediaMessages.splice(0)
    .filter(value => value !== '__media_test_barrier__'));
}

async function checkEmbeddedProviders(browser, target) {
  const context = await browser.newContext({viewport: {width: 1440, height: 900}});
  let redirectPlayer = false;
  const errors = [];
  const cases = [
    {name: 'youtube', url: 'https://www.youtube.com/embed/42', provider: 'youtube'},
    {name: 'youtube-bare', url: 'https://youtube.com/embed/42', provider: 'youtube'},
    {name: 'youtube-query-hash', url: 'https://www.youtube.com/embed/42?start=4#chapter', provider: 'youtube'},
    {name: 'youtube-case-port', url: 'HTTPS://WWW.YOUTUBE.COM:443/embed/42?enablejsapi=0', provider: 'youtube'},
    {name: 'vimeo', url: 'https://player.vimeo.com/video/42?color=ffffff#chapter', provider: 'vimeo'},
    {name: 'protocol-relative', url: '//www.youtube.com/embed/42',
      provider: new URL(target).protocol === 'https:' ? 'youtube' : undefined},
    {name: 'similar-host', url: 'https://notyoutube.com/embed/42'},
    {name: 'host-suffix', url: 'https://www.youtube.com.evil.invalid/embed/42'},
    {name: 'path-bait', url: 'https://example.invalid/path/youtube.com/embed/42'},
    {name: 'query-bait', url: 'https://example.invalid/?next=https://player.vimeo.com/video/42'},
    {name: 'fragment-bait', url: 'https://example.invalid/#youtube.com/embed/42'},
    {name: 'other-path', url: 'https://www.youtube.com/watch?v=42'},
    {name: 'vimeo-other-path', url: 'https://player.vimeo.com/channel/42'},
    {name: 'other-port', url: 'https://player.vimeo.com:8443/video/42'},
    {name: 'local-relative', url: '../media/frame.html?value=1#chapter'},
    {name: 'fragment', url: 'https://www.youtube.com/embed/42', provider: 'youtube', fragment: true},
    {name: 'autoplay-disabled', url: 'https://player.vimeo.com/video/42', provider: 'vimeo', autoplay: false},
    {name: 'ignore-pause', url: 'https://www.youtube.com/embed/42', provider: 'youtube', ignore: true},
    {name: 'redirect-origin', url: 'https://www.youtube.com/embed/42', provider: 'youtube', redirect: true},
    // Credentials and non-network URLs need no navigation to test classification.
    {name: 'userinfo-host', url: 'https://youtube.com@evil.invalid/embed/42', formatOnly: true},
    {name: 'userinfo-provider', url: 'https://user@www.youtube.com/embed/42', formatOnly: true},
    {name: 'http-provider', url: 'http://player.vimeo.com/video/42',
      formatOnly: new URL(target).protocol === 'https:'},
    {name: 'opaque-origin', url: 'data:text/html,youtube.com/embed/42', formatOnly: true},
  ];
  try {
    await context.route('**/*', route => {
      const request = route.request();
      if (request.isNavigationRequest() && request.frame().parentFrame()) {
        const url = new URL(request.url());
        if (redirectPlayer && url.hostname === 'www.youtube.com') {
          return route.fulfill({status: 302, headers: {location: 'https://redirect.invalid/frame'}});
        }
        return route.fulfill({contentType: 'text/html', body: frameDocument});
      }
      return route.continue();
    });
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(target + '/deck/');
    await page.waitForFunction(() => typeof deck !== 'undefined' && deck.isReady());
    await page.evaluate(() => {
      deck.destroy();
      document.querySelector('.reveal').remove();
      addEventListener('message', event => {
        if (event.source === document.querySelector('#media-case')?.contentWindow &&
            event.data?.kind === 'media-case-message') {
          window.mediaMessages.push(event.data.message);
        }
      });
    });
    for (const test of cases) {
      redirectPlayer = test.redirect === true;
      const initial = await page.evaluate(async test => {
        if (window.mediaDeck) {
          window.mediaDeck.destroy();
          window.mediaRoot.remove();
        }
        window.mediaMessages = [];
        const root = document.createElement('div');
        root.className = 'reveal';
        const slides = document.createElement('div');
        slides.className = 'slides';
        const idle = document.createElement('section');
        idle.textContent = 'Idle slide';
        const active = document.createElement('section');
        const frame = document.createElement('iframe');
        frame.id = 'media-case';
        frame.title = test.name;
        frame.setAttribute('data-src', test.url);
        if (test.fragment) frame.classList.add('fragment');
        if (test.ignore) frame.setAttribute('data-ignore', '');
        active.appendChild(frame);
        slides.append(idle, active);
        root.appendChild(slides);
        document.body.appendChild(root);
        window.mediaRoot = root;
        window.mediaDeck = new Reveal(root, {embedded: true, controls: false, progress: false,
          hash: false, transition: 'none', autoPlayMedia: test.autoplay !== false,
          viewDistance: 1, preloadIframes: false});
        await window.mediaDeck.initialize();
        const formatted = frame.getAttribute('data-src');
        window.mediaDeck.sync();
        return {formatted, repeated: frame.getAttribute('data-src'), src: frame.getAttribute('src')};
      }, test);
      assert.equal(initial.src, null, test.name + ': inactive iframe loaded');
      assert.equal(initial.repeated, initial.formatted, test.name + ': formatting is not idempotent');
      if (test.provider) {
        const original = new URL(test.url, target + '/deck/');
        const formatted = new URL(initial.formatted, target + '/deck/');
        const parameter = test.provider === 'youtube' ? 'enablejsapi' : 'api';
        assert.equal(formatted.searchParams.get(parameter), '1', test.name + ': API query missing');
        assert.equal(formatted.hash, original.hash, test.name + ': fragment changed');
        for (const [key, value] of original.searchParams) {
          if (key !== parameter) assert.equal(formatted.searchParams.get(key), value);
        }
      } else {
        assert.equal(initial.formatted, test.url, test.name + ': non-provider URL was modified');
      }
      if (test.formatOnly) continue;
      const destination = test.redirect ? 'https://redirect.invalid/frame'
        : new URL(initial.formatted, page.url()).href;
      // Observe this navigation's actual target and load, never the initial about:blank.
      const loaded = page.waitForEvent('framenavigated', frame =>
        frame.parentFrame() === page.mainFrame() && frame.url() === destination)
        .then(frame => frame.waitForLoadState('load'));
      await page.evaluate(() => window.mediaDeck.slide(1));
      if (test.fragment) {
        assert.equal(await page.locator('#media-case').getAttribute('src'), null,
          'hidden fragment iframe must remain lazy');
        await page.evaluate(() => window.mediaDeck.nextFragment());
      }
      await loaded;
      const started = (await drainMessages(page)).map(decodeMessage);
      const play = test.provider === 'youtube'
        ? {event: 'command', func: 'playVideo', args: ''} : {method: 'play'};
      assert.deepEqual(started, test.redirect ? []
        : test.provider && test.autoplay !== false ? [play] : ['slide:start'], test.name + ': start');
      await page.evaluate(() => window.mediaDeck.stopEmbeddedContent());
      const stopped = (await drainMessages(page)).map(decodeMessage);
      const pause = test.provider === 'youtube'
        ? {event: 'command', func: 'pauseVideo', args: ''} : {method: 'pause'};
      assert.deepEqual(stopped, test.provider && !test.ignore && !test.redirect
        ? ['slide:stop', pause] : ['slide:stop'], test.name + ': stop');
      console.log('PASS media provider ' + test.name);
    }
    assert.deepEqual(errors, []);
    return {suite: 'embedded-media-providers', result: 'pass', cases: cases.length,
      lifecycleCases: cases.filter(test => !test.formatOnly).length};
  } finally {
    await context.close();
  }
}

module.exports = {checkEmbeddedProviders};
