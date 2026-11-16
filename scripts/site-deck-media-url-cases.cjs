// Exercise URL activation through the actual Reveal controller and browser DOM.
const assert = require('node:assert/strict');

async function scenario(page, options) {
  await page.evaluate(async options => {
    if (window.urlDeck) {
      window.urlDeck.destroy();
      window.urlRoot.remove();
    }
    const root = document.createElement('div');
    root.className = 'reveal';
    const slides = document.createElement('div');
    slides.className = 'slides';
    const idle = document.createElement('section');
    idle.textContent = 'Idle';
    const active = document.createElement('section');
    const add = (tag, id, value, parent = active) => {
      const element = document.createElement(tag);
      element.id = id;
      element.setAttribute('data-url-test', '');
      element.setAttribute('data-src', value);
      parent.appendChild(element);
      return element;
    };
    if (options.group === 'body') {
      const values = options.values || {};
      for (const tag of ['img', 'video', 'audio'])
        add(tag, tag, values[tag] ?? options.url);
      for (const tag of ['video', 'audio']) {
        const media = document.createElement(tag);
        active.appendChild(media);
        if (tag === 'video' && options.mixedSources)
          add('source', 'invalid-sibling-source', 'javascript:parent.mediaUrlExecuted+=1', media);
        add('source', tag + '-source', values[tag + '-source'] ?? options.url, media);
      }
      add('iframe', 'url-frame', values.frame ?? options.url);
      const fragment = document.createElement('div');
      fragment.className = 'fragment';
      active.appendChild(fragment);
      add('iframe', 'url-fragment', values.fragment ?? options.url, fragment);
    } else {
      active.setAttribute('data-background-' + options.group, options.url);
      if (options.group === 'video') {
        active.setAttribute('data-background-video-loop', '');
        active.setAttribute('data-background-video-muted', '');
      }
    }
    const good = document.createElement('img');
    good.id = 'url-good-neighbor';
    good.setAttribute('data-src', './fixtures/good.svg?neighbor=1#pixel');
    active.appendChild(good);
    slides.append(idle, active);
    root.appendChild(slides);
    document.body.appendChild(root);
    window.urlRoot = root;
    window.urlActive = active;
    window.urlDeck = new Reveal(root, {embedded: true, controls: false, progress: false,
      hash: false, transition: 'none', autoPlayMedia: false,
      viewDistance: options.nearby ? 3 : 1, preloadIframes: options.preload});
    await window.urlDeck.initialize();
  }, options);
}

async function rejected(page, name, group, value) {
  const nodes = await page.evaluate(group => {
    const root = group === 'body' ? window.urlActive : window.urlActive.slideBackgroundContentElement;
    return Array.from(root.querySelectorAll(group === 'body' ? '[data-url-test]' : 'iframe, source'),
      node => ({id: node.id || node.tagName, src: node.getAttribute('src'),
        lazy: node.hasAttribute('data-lazy-loaded'), pending: node.getAttribute('data-src')}));
  }, group);
  if (group === 'video') assert.equal(nodes.length, 0, name + ': rejected background source was created');
  for (const node of nodes) {
    assert.equal(node.src, null, name + '/' + node.id + ': rejected URL reached src');
    assert.equal(node.lazy, false, name + '/' + node.id + ': rejected URL marked loaded');
    assert.equal(node.pending, value, name + '/' + node.id + ': pending URL changed');
  }
  await page.waitForFunction(() => {
    const image = document.querySelector('#url-good-neighbor');
    return image.complete && image.naturalWidth === 1;
  });
  assert.equal(await page.evaluate(() => window.mediaUrlExecuted), 0, name + ': URL script executed');
}

async function frameLoaded(page, selector, destination, activate, newNavigation = false) {
  const element = await page.$(selector);
  const frame = await element.contentFrame();
  const loaded = newNavigation
    ? page.waitForEvent('framenavigated', current => current === frame && current.url() === destination)
      .then(current => current.waitForLoadState('load'))
    : frame.waitForURL(destination, {waitUntil: 'load'});
  await activate();
  await loaded;
}

async function noRepeatedSrc(page, selector, activate) {
  await page.evaluate(selector => {
    window.urlSrcWrites = [];
    window.urlObserver = new MutationObserver(records => {
      window.urlSrcWrites.push(...records.map(record => record.target.getAttribute('src')));
    });
    window.urlObserver.observe(document.querySelector(selector), {attributes: true, attributeFilter: ['src']});
  }, selector);
  await activate();
  const writes = await page.evaluate(async () => {
    await Promise.resolve();
    window.urlObserver.disconnect();
    return window.urlSrcWrites;
  });
  assert.deepEqual(writes, [], selector + ': repeated activation rewrote src');
}

async function checkMediaUrls(browser, target) {
  const context = await browser.newContext({viewport: {width: 1440, height: 900}});
  const errors = [];
  let negativeCases = 0;
  try {
    await context.route('**/*', route => {
      const request = route.request();
      if (request.isNavigationRequest() && request.frame().parentFrame())
        return route.fulfill({contentType: 'text/html', body: '<!doctype html><title>URL fixture</title>'});
      if (new URL(request.url()).pathname.includes('/fixtures/')) {
        if (request.resourceType() === 'image') return route.fulfill({contentType: 'image/svg+xml',
          body: '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>'});
        // URL/lifecycle assertions do not claim to decode or play a real video service.
        return route.fulfill({status: 204, body: ''});
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
      window.mediaUrlExecuted = 0;
    });
    const invalid = [
      ['javascript', 'javascript:parent.mediaUrlExecuted+=1;void 0'],
      ['mixed-case', 'JaVaScRiPt:parent.mediaUrlExecuted+=1;void 0'],
      ['control-character', 'java\nscript:parent.mediaUrlExecuted+=1;void 0'],
      ['leading-control', '\u0000javascript:parent.mediaUrlExecuted+=1;void 0'],
      ['data', 'data:text/html,<script>parent.mediaUrlExecuted+=1</script>'],
      ['vbscript', 'vbscript:msgbox(1)'],
      ['blob', 'blob:https://example.invalid/media-fixture'],
      ['file', 'file:///nonexistent-oasis7-media-fixture.html'],
      ['malformed', 'https://[invalid'],
      ['empty', ''],
      ['whitespace', ' \t\r\n '],
    ];
    for (const [name, url] of invalid) {
      for (const {group, preload} of [{group: 'body', preload: true}, {group: 'body', preload: false},
        {group: 'video', preload: false}, {group: 'iframe', preload: true}, {group: 'iframe', preload: false}]) {
        // Background video uses a comma-separated source list, not a data-URL syntax.
        const value = group === 'video' && name === 'data' ? 'data:text/html' : url;
        const label = name + '/' + group + '/preload=' + preload;
        await scenario(page, {group, preload, url: value});
        await page.evaluate(() => window.urlDeck.loadSlide(window.urlActive));
        await rejected(page, label + '/preload', group, value);
        await page.evaluate(() => window.urlDeck.slide(1));
        if (group === 'body') await page.evaluate(() => window.urlDeck.nextFragment());
        await rejected(page, label + '/active', group, value);
        await page.evaluate(() => {
          window.urlDeck.slide(0);
          window.urlDeck.slide(1);
        });
        if (group === 'body') await page.evaluate(() => window.urlDeck.nextFragment());
        await rejected(page, label + '/reenter', group, value);
        negativeCases += 1;
      }
      console.log('PASS media URL rejects ' + name + ' across body, sources and backgrounds');
    }

    const base = page.url();
    const values = {img: '../fixtures/image.svg?x=1#pixel', video: './fixtures/video.webm?x=1',
      audio: '/fixtures/audio.wav#sample', 'video-source': './fixtures/source.webm?x=1',
      'audio-source': './fixtures/source.wav', frame: './fixtures/frame.html?x=1#frame',
      fragment: '//example.invalid/fixtures/fragment.html?x=1#frame'};
    for (const preload of [false, true]) {
      if (preload) values.fragment = 'https://example.invalid/fixtures/fragment.html?x=1#frame';
      await scenario(page, {group: 'body', preload, nearby: true, mixedSources: true, url: '', values});
      if (!preload) assert.equal(await page.locator('#url-frame').getAttribute('src'), null);
      await frameLoaded(page, '#url-frame', new URL(values.frame, base).href,
        () => page.evaluate(() => window.urlDeck.slide(1)));
      for (const id of ['img', 'video', 'audio', 'video-source', 'audio-source']) {
        assert.equal(await page.locator('#' + id).getAttribute('src'), new URL(values[id], base).href);
        assert.equal(await page.locator('#' + id).getAttribute('data-src'), null);
        assert.equal(await page.locator('#' + id).getAttribute('data-lazy-loaded'), '');
      }
      assert.equal(await page.locator('#invalid-sibling-source').getAttribute('src'), null);
      assert.equal(await page.locator('#invalid-sibling-source').getAttribute('data-lazy-loaded'), null);
      await noRepeatedSrc(page, '#url-frame', () => page.evaluate(() => {
        window.urlDeck.loadSlide(window.urlActive);
        window.urlDeck.startEmbeddedContent();
        window.urlDeck.startEmbeddedContent();
      }));
      if (!preload) assert.equal(await page.locator('#url-fragment').getAttribute('src'), null);
      await frameLoaded(page, '#url-fragment', new URL(values.fragment, base).href,
        () => page.evaluate(() => window.urlDeck.nextFragment()));
      await page.evaluate(() => window.urlDeck.prevFragment());
      if (!preload) assert.equal(await page.locator('#url-fragment').getAttribute('src'), null,
        'hidden fragment must unload even within viewDistance');
      await page.evaluate(() => window.urlDeck.slide(0));
      assert.equal(await page.locator('#url-frame').getAttribute('src'), preload ? new URL(values.frame, base).href : null);
      await page.evaluate(() => window.urlDeck.unloadSlide(window.urlActive));
      assert.equal(await page.locator('#url-frame').getAttribute('src'), null);
      await frameLoaded(page, '#url-frame', new URL(values.frame, base).href,
        () => page.evaluate(() => window.urlDeck.slide(1)), true);
      console.log('PASS media URL body lifecycle preload=' + preload);
    }

    await scenario(page, {group: 'video', preload: false,
      url: './fixtures/one.webm?x=1,javascript:parent.mediaUrlExecuted+=1,/fixtures/two.mp4#tail'});
    await page.evaluate(() => window.urlDeck.slide(1));
    const backgroundVideo = await page.evaluate(() => {
      const video = window.urlActive.slideBackgroundContentElement.querySelector('video');
      return {loop: video.loop, muted: video.muted,
        sources: Array.from(video.querySelectorAll('source'), source => source.getAttribute('src'))};
    });
    assert.deepEqual(backgroundVideo, {loop: true, muted: true,
      sources: [new URL('./fixtures/one.webm?x=1', base).href, new URL('/fixtures/two.mp4#tail', base).href]});

    for (const preload of [false, true]) {
      const url = './fixtures/background.html?x=1#frame';
      const selector = '.slide-background iframe';
      await scenario(page, {group: 'iframe', preload, nearby: true, url});
      await page.evaluate(() => window.urlDeck.slide(1));
      await frameLoaded(page, selector, new URL(url, base).href, async () => {});
      await noRepeatedSrc(page, selector, () => page.evaluate(preload => {
        window.urlDeck.loadSlide(window.urlActive);
        // A preloaded background stays alive across a public slide round trip;
        // re-entry exercises its active path without a legitimate unload/reload.
        if (preload) {
          window.urlDeck.slide(0);
          window.urlDeck.slide(1);
        }
      }, preload));
      await page.evaluate(() => window.urlDeck.slide(0));
      const afterExit = await page.evaluate(() =>
        window.urlActive.slideBackgroundContentElement.querySelector('iframe').getAttribute('src'));
      assert.equal(afterExit, preload ? new URL(url, base).href : null);
      await page.evaluate(() => window.urlDeck.unloadSlide(window.urlActive));
      assert.equal(await page.evaluate(() =>
        window.urlActive.slideBackgroundContentElement.querySelector('iframe').getAttribute('src')), null);
      await frameLoaded(page, selector, new URL(url, base).href,
        () => page.evaluate(() => window.urlDeck.slide(1)), true);
      console.log('PASS media URL background iframe lifecycle preload=' + preload);
    }
    const deferredBackground = './fixtures/background-autoplay.html?autoplay=1';
    await scenario(page, {group: 'iframe', preload: true, url: deferredBackground});
    await page.evaluate(() => window.urlDeck.loadSlide(window.urlActive));
    const backgroundFrame = '.slide-background iframe';
    assert.equal(await page.locator(backgroundFrame).getAttribute('src'), null,
      'autoplay background must retain its existing deferred preload rule');
    await frameLoaded(page, backgroundFrame, new URL(deferredBackground, base).href,
      () => page.evaluate(() => window.urlDeck.slide(1)));
    await scenario(page, {group: 'iframe', preload: true, url: './fixtures/excluded.html'});
    await page.evaluate(() => window.urlDeck.loadSlide(window.urlActive, {excludeIframes: true}));
    assert.equal(await page.locator('.slide-background iframe').count(), 0);
    assert.equal(await page.evaluate(() => window.mediaUrlExecuted), 0);
    assert.deepEqual(errors, []);
    return {suite: 'media-url-activation', result: 'pass', negativeCases,
      positiveCases: 7, decodingOrExternalPlayback: 'not-tested'};
  } finally { await context.close(); }
}

module.exports = {checkMediaUrls};
