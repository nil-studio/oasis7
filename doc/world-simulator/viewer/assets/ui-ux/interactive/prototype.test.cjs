/*
 * UI/UE prototype regression checks for #5527.
 *
 * From the repository root, after installing the existing Viewer dev dependencies:
 *   node doc/world-simulator/viewer/assets/ui-ux/interactive/prototype.test.cjs
 *
 * JSDOM is resolved normally (including NODE_PATH), then from the existing Viewer
 * package. No dependency installation, game service, HTTP server or browser is used.
 * Results are JSON on stdout; redirect stdout explicitly to retain a report.
 * This test never writes source files or report files itself.
 *
 * These are source/DOM state checks. The dialog shim is not native modality,
 * focus trapping, browser layout, actual IME input or assistive-technology evidence.
 */
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { createRequire } = require('node:module');

let jsdomPath;
try {
  jsdomPath = require.resolve('jsdom');
} catch (error) {
  if (error.code !== 'MODULE_NOT_FOUND') throw error;
  const viewerRequire = createRequire(path.resolve(__dirname,
    '../../../../../../crates/oasis7_viewer/package.json'));
  jsdomPath = viewerRequire.resolve('jsdom');
}
const { JSDOM } = require(jsdomPath);
const html = fs.readFileSync(path.join(__dirname, 'index.html'), 'utf8');
const js = fs.readFileSync(path.join(__dirname, 'prototype.js'), 'utf8');
const i18n = fs.readFileSync(path.join(__dirname, 'prototype-i18n.js'), 'utf8');
const checks = [];
const activeContexts = new Set();

// Add a read-only observer to an in-memory copy of the script. The prototype file
// stays unchanged and does not expose private state to its actual browser UI.
const stateDeclaration = '  let state;';
assert.equal(js.split(stateDeclaration).length, 2, 'Expected one prototype state declaration');
const observedScript = js.replace(stateDeclaration,
  `${stateDeclaration} window.__businessSnapshot = () => JSON.stringify(state);`);

function context(route, fixture) {
  const dom = new JSDOM(html, {
    url: `https://design-fixture.invalid/#${route}`,
    runScripts: 'outside-only',
    pretendToBeVisual: true,
  });
  const w = dom.window;
  const d = w.document;
  const scriptErrors = [];
  const close = () => {
    if (!activeContexts.delete(close)) return;
    dom.window.close();
    assert.deepEqual(scriptErrors, [], 'Unexpected prototype script errors');
  };
  activeContexts.add(close);
  w.addEventListener('error', (event) => scriptErrors.push(String(event.error || event.message)));
  // JSDOM does not implement native dialogs. Only callback and open-state behavior
  // are exercised here; real-browser checks must cover modality and focus trapping.
  w.HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  w.HTMLDialogElement.prototype.close = function () {
    this.removeAttribute('open');
    this.dispatchEvent(new w.Event('close'));
  };
  w.eval(i18n);
  w.eval(observedScript);
  const id = (value) => d.getElementById(value);
  const select = (name, value) => {
    id(name).value = value;
    id(name).dispatchEvent(new w.Event('change', { bubbles: true }));
  };
  if (fixture) select('fixture', fixture);
  const click = (action) => {
    const element = d.querySelector(`[data-action="${action}"]`);
    assert(element, `Missing action: ${action}`);
    assert(!element.disabled, `Disabled action: ${action}`);
    element.focus();
    element.click();
  };
  const input = (name, value) => {
    id(name).value = value;
    id(name).dispatchEvent(new w.Event('input', { bubbles: true }));
  };
  const text = () => id('drawer-content').textContent;
  return { w, d, select, click, id, input, text, close };
}

function check(name, fn) {
  let failure;
  try {
    fn();
  } catch (error) {
    failure = error;
  } finally {
    // An assertion failure must not leave a JSDOM window or queued task behind.
    for (const close of [...activeContexts]) {
      try { close(); } catch (error) { failure ||= error; }
    }
  }
  checks.push(failure
    ? { name, result: 'FAIL', error: failure.stack || String(failure) }
    : { name, result: 'pass' });
}

check('Entry: enter, multiline draft, explicit send, original goal readback',()=>{const c=context('entry');c.click('enter');c.id('nav-command').click();c.input('chat-draft','先核查燃料\n再规划生产');c.id('chat-draft').dispatchEvent(new c.w.KeyboardEvent('keydown',{key:'Enter',bubbles:true}));assert(c.text().includes('尚未确认目标'));c.click('chat-send');assert(c.text().includes('尚未确认目标'));c.click('chat-query');assert(c.text().includes('先核查燃料'));assert(c.text().includes('当前活动'));c.close();});
check('Chat: A original query does not overwrite C goal',()=>{const c=context('entry','identities');c.click('chat-query');assert(c.text().includes('尚未确认目标'));assert(c.id('receipt').textContent.includes('Agent A'));c.close();});
check('IME guard: Escape during composition leaves nonmodal panel open',()=>{const c=context('entry','identities');c.id('chat-draft').dispatchEvent(new c.w.KeyboardEvent('keydown',{key:'Escape',isComposing:true,bubbles:true}));assert(!c.id('drawer').hidden);c.id('chat-draft').dispatchEvent(new c.w.KeyboardEvent('keydown',{key:'Escape',bubbles:true}));assert(c.id('drawer').hidden);c.close();});
check('Prompt noop: clear retains effective B and version 7',()=>{const c=context('prompt','noop');c.click('clear-short');c.click('prompt-apply');assert(c.text().includes('Profile 版本仍为 v7'));assert(c.text().includes('short-B'));assert(c.text().includes('没有 mutation'));c.close();});
check('Prompt blank normalization: whitespace becomes no-change',()=>{const c=context('prompt','noop');c.input('prompt-short',' \n ');c.click('prompt-apply');assert(c.text().includes('Profile 版本仍为 v7'));assert(c.text().includes('short-B'));c.close();});
check('Prompt override clear: C persists until matching effective read, then A',()=>{const c=context('prompt','override');c.click('clear-short');assert(c.id('prompt-impact').textContent.includes('short-C → short-A'));c.click('prompt-apply');assert(c.text().includes('执行器待读回'));assert(c.text().includes('short-C'));c.click('prompt-effective');assert(!c.text().includes('short-C'));assert(c.text().includes('short-A'));c.close();});
check('Prompt indirect impact is visible without preview and refreshes on input',()=>{const c=context('prompt','indirect');assert(c.id('prompt-impact').textContent.includes('short-B → short-A'));c.input('prompt-system','system-E');assert(c.id('prompt-impact').textContent.includes('system-A → system-E'));assert(!c.id('prompt-impact').textContent.includes('system-D'));c.close();});
check('Skip link preserves Prompt draft and route',()=>{const c=context('prompt','noop');c.input('prompt-short','独立草稿');c.id('skip-to-stage').click();assert.equal(c.id('prompt-short').value,'独立草稿');assert.equal(c.w.location.hash,'#prompt');assert.equal(c.d.activeElement.id,'stage');c.close();});
check('Preset is draft only and confirms dirty replacement',()=>{const c=context('prompt','indirect');c.click('prompt-preset');assert(c.id('confirm-dialog').open);c.id('cancel-confirm').click();assert.equal(c.id('prompt-system').value,'system-D');c.click('prompt-preset');c.id('accept-confirm').click();assert.equal(c.id('prompt-system').value,'preset-P / system');assert(c.text().includes('short-B'));assert(c.id('receipt').hidden);c.close();});
check('Stale compare is readonly and rebase preserves draft until apply',()=>{const c=context('prompt','stale');c.click('prompt-compare');assert.equal(c.id('prompt-short').value,'short-D');assert(c.text().includes('short-C2'));c.click('prompt-rebase');assert.equal(c.id('prompt-short').value,'short-D');assert(c.id('receipt').hidden);c.click('prompt-apply');assert(c.text().includes('v9'));c.close();});
check('Stale reload cancel keeps draft, accept replaces without applying',()=>{const c=context('prompt','stale');c.click('prompt-reload');c.id('cancel-confirm').click();assert.equal(c.id('prompt-short').value,'short-D');c.click('prompt-reload');c.id('accept-confirm').click();assert.equal(c.id('prompt-short').value,'short-C2');assert(c.id('receipt').hidden);c.close();});
check('Repeated stale read failure keeps draft and ends lookup',()=>{const c=context('prompt','stale-read-failed');c.click('prompt-read-again');c.click('prompt-read-again');assert.equal(c.id('safe-draft').value,'short-D');assert(c.id('notice').textContent.includes('重读已结束'));c.close();});
check('Unknown Apply remains unknown after current profile v8 read',()=>{const c=context('prompt','unknown');c.click('prompt-latest');assert(c.id('receipt').textContent.includes('原 Apply 结果未知'));assert(c.text().includes('已读到 v8'));c.close();});
check('Prompt original A survives confirmed switch to C and C noop',()=>{const c=context('prompt','unknown');c.id('nav-targets').click();c.click('observe-C');c.click('use-observed');assert(c.id('confirm-dialog').open);c.id('cancel-confirm').click();assert(c.id('receipt').textContent.includes('结果未知'));c.click('use-observed');c.id('accept-confirm').click();assert(c.id('receipt').textContent.includes('Agent A'));assert(c.id('receipt').textContent.includes('结果未知'));c.id('nav-command').click();c.click('prompt-apply');assert(c.id('receipt').textContent.includes('Agent C'));c.click('prompt-history');assert(c.text().includes('Agent A'));assert(c.text().includes('原 Apply 结果未知'));assert(c.text().includes('Agent C'));c.close();});
check('Revocation hides all private Prompt sources and actions',()=>{const c=context('prompt','revoked');assert(!c.text().includes('system-A'));assert(!c.text().includes('short-B'));assert(!c.d.querySelector('[data-action="prompt-apply"]'));assert(c.id('safe-draft'));c.close();});
check('Blocked quote has no successful duration and cannot magically add fuel',()=>{const c=context('industry','blocked');c.click('industry-preview');assert(c.text().includes('尚未取得'));c.click('industry-recheck');assert(c.text().includes('可用 2 carbon_fuel'));assert(c.d.querySelector('[data-action="industry-submit"]').disabled);assert(c.id('receipt').hidden);c.close();});
check('External supply update requires recheck before explicit submit',()=>{const c=context('industry','blocked');c.select('supply','3');assert(c.d.querySelector('[data-action="industry-submit"]').disabled);c.click('industry-recheck');assert(!c.d.querySelector('[data-action="industry-submit"]').disabled);assert(c.id('receipt').hidden);c.click('industry-submit');assert(c.id('receipt').textContent.includes('结果未知'));assert(c.text().includes('提交前'));assert(!c.d.querySelector('[data-action="industry-submit"]'));c.close();});
check('Industry query failure and mismatched job preserve original unknown',()=>{const c=context('industry','unknown');c.select('readback','unavailable');c.click('industry-query');assert(c.id('receipt').textContent.includes('结果未知'));assert(c.text().includes('查询暂时失败'));c.select('readback','mismatch');c.click('industry-query');assert(c.id('receipt').textContent.includes('结果未知'));assert(c.id('notice').textContent.includes('另一个 job'));c.close();});
check('Industry settled does not invent byproduct or current inventory',()=>{const c=context('industry','unknown');c.click('industry-query');assert(c.text().includes('9 iron_ingot'));assert(c.text().includes('当前摘要未发布'));assert(c.text().includes('本笔产出 9'));assert(c.text().includes('这不是当前库存余额'));assert(!c.text().includes('当前可见 9'));c.close();});
check('Full receipt fixture attributes slag to matching original',()=>{const c=context('industry','full-receipt');assert(c.text().includes('3 slag'));assert(c.text().includes('RecipeCompletionReceiptV1'));assert(c.text().includes('job 17'));c.close();});
check('Rejected original is distinct from unknown and has no automatic resubmit',()=>{const c=context('industry','unknown');c.select('readback','rejected');c.click('industry-query');assert(c.id('receipt').textContent.includes('明确拒绝'));assert(c.text().includes('已耗材料与费用未发布'));assert(!c.d.querySelector('[data-action="industry-submit"]'));c.close();});
check('World receipt query retains focus at stable action location',()=>{const c=context('industry','unknown');c.id('close-drawer').click();c.id('receipt-action').focus();c.id('receipt-action').click();assert.equal(c.d.activeElement.id,'receipt-action');assert(c.id('receipt').textContent.includes('已结算'));c.close();});
check('All industry exit paths preserve historical original job',()=>{for(const close of ['nav-world','close-drawer','Escape']){const c=context('industry','settled');if(close==='Escape')c.id('drawer-title').dispatchEvent(new c.w.KeyboardEvent('keydown',{key:'Escape',bubbles:true}));else c.id(close).click();c.id('nav-command').click();assert(c.text().includes('回访 · 历史结果'));assert(c.text().includes('job 17'));c.close();}});
check('Signout clears private industrial result; reentry is fresh and controllable',()=>{const c=context('industry','settled');c.id('account-button').click();c.click('signout');assert(c.id('receipt').hidden);assert(!c.text().includes('iron_ingot'));c.click('enter');c.id('nav-command').click();assert(c.text().includes('先补足碳燃料'));assert(!c.text().includes('第一批铁锭已产出'));c.select('supply','3');c.click('industry-recheck');assert(!c.d.querySelector('[data-action="industry-submit"]').disabled);c.close();});
check('Renderer unavailable removes old world art, keeps text route',()=>{const c=context('entry','renderer');assert(c.id('world-art').hidden);assert(!c.id('renderer-message').hidden);c.id('nav-world').click();assert(c.id('world-art').hidden);c.click('retry-renderer');assert(!c.id('world-art').hidden);c.close();});

check('Position fixture: observing A or C replaces unknown detail without changing the draft',()=>{const c=context('entry','position');for(const agent of ['A','C']){c.click(`observe-${agent}`);assert(c.text().includes(`正在查看 ${agent}`));assert(!c.d.querySelector('.card.warn'));assert(c.d.querySelector('[data-action="use-observed"]'));assert(c.id('world-context').textContent.includes('草稿 A'));}c.close();});
check('Dense fixture: unknown target removes collection highlight; A/C restore one visible cluster',()=>{const c=context('entry','dense');c.click('observe-unknown');assert([...c.d.querySelectorAll('.actor')].filter(el=>!el.hidden).every(el=>el.style.outline==='none'));for(const agent of ['A','C']){c.click(`observe-${agent}`);const visible=[...c.d.querySelectorAll('.actor')].filter(el=>!el.hidden);assert.equal(visible.length,1);assert.equal(visible[0].textContent,'3');assert.notEqual(visible[0].style.outline,'none');}c.close();});
check('Revoked control: Targets only observes and preserves the retained local draft',()=>{const c=context('prompt','revoked');const draft=c.id('safe-draft').value;c.id('nav-targets').click();for(const agent of ['A','C']){c.click(`observe-${agent}`);assert(!c.text().includes('本示例可指导'));assert(!c.d.querySelector('[data-action="use-observed"]'));assert(c.text().includes('不能向它提交指令'));}c.id('nav-command').click();assert.equal(c.id('safe-draft').value,draft);c.close();});
check('Session reset synchronizes fixture, readback and supply before reentry',()=>{for(const fixture of ['settled','query-failed']){const c=context('industry',fixture);c.select('readback','mismatch');c.id('account-button').click();c.click('signout');assert.equal(c.id('fixture').value,'blocked');assert.equal(c.id('readback').value,'settled');assert.equal(c.id('supply').value,'2');c.click('enter');c.id('nav-command').click();assert(c.text().includes('可用 2 carbon_fuel'));assert(c.d.querySelector('[data-action="industry-submit"]').disabled);assert.equal(c.id('supply').value,'2');c.close();}});
check('English entry journey keeps arbitrary user text and requires explicit send/readback',()=>{const c=context('entry');c.select('locale','en');c.click('enter');c.id('nav-command').click();const raw='当前没有控制权限 <b>unchanged</b> &\n用户原文';c.input('chat-draft',raw);const before=c.w.__businessSnapshot();for(const mode of ['long','missing','normal']){c.select('copy-mode',mode);assert.equal(c.w.__businessSnapshot(),before);assert.equal(c.id('chat-draft').value,raw);}assert(!c.d.querySelector('#chat-draft b'));c.click('chat-send');assert(c.id('receipt').textContent.includes('awaiting confirmation'));c.click('chat-query');assert(c.text().includes(raw));assert(c.text().includes('Current activity'));c.close();});
check('Locale/copy switches preserve exact Prompt edit buffers, selection and normalized state',()=>{const c=context('prompt','noop');const raw='  当前目标 <tag> & English\n  ';c.input('prompt-short',raw);c.id('prompt-short').setSelectionRange(2,9,'backward');const before=c.w.__businessSnapshot();for(const locale of ['en','zh-CN','en'])for(const mode of ['long','missing','normal']){c.select('locale',locale);c.select('copy-mode',mode);assert.equal(c.w.__businessSnapshot(),before);assert.equal(c.id('prompt-short').value,raw);assert.equal(c.id('prompt-short').selectionStart,2);assert.equal(c.id('prompt-short').selectionEnd,9);}c.close();});
check('Unknown Apply with newer profile remains original A/unknown through every display mode',()=>{const c=context('prompt','unknown');c.click('prompt-latest');const before=c.w.__businessSnapshot();for(const locale of ['en','zh-CN'])for(const mode of ['normal','long','missing']){c.select('locale',locale);c.select('copy-mode',mode);assert.equal(c.w.__businessSnapshot(),before);assert(c.id('receipt').textContent.includes('prompt-demo-08'));assert(c.id('receipt').textContent.includes('Agent A'));assert.equal(c.id('prompt-system').value,'system-D');assert(c.id('prompt-system').readOnly);assert(c.text().includes('v8'));}c.close();});
check('Existing confirm dialog localizes without accepting, canceling or changing its pending action',()=>{const c=context('prompt','stale');c.click('prompt-reload');const before=c.w.__businessSnapshot();c.select('locale','en');c.select('copy-mode','long');assert(c.id('confirm-dialog').open);assert.equal(c.id('confirm-title').textContent,'Discard the draft and reread?');assert.equal(c.w.__businessSnapshot(),before);assert.equal(c.id('prompt-short').value,'short-D');c.id('accept-confirm').click();assert(!c.id('confirm-dialog').open);assert.equal(c.id('prompt-short').value,'short-C2');assert(c.id('receipt').hidden);c.close();});
check('English Prompt apply preserves raw source tokens and separates profile/driver results',()=>{const c=context('prompt','override');c.select('locale','en');c.click('clear-short');assert(c.id('prompt-impact').textContent.includes('short-C → short-A'));c.click('prompt-apply');assert(c.text().includes('driver readback pending'));assert(c.text().includes('short-C'));c.click('prompt-effective');assert(c.text().includes('Applied and read back'));assert(c.text().includes('short-A'));assert(!c.text().includes('short-C'));c.close();});
check('English industry flow keeps insufficient supply, unknown/mismatch and published result boundaries',()=>{const c=context('industry','blocked');c.select('locale','en');c.select('copy-mode','long');assert(c.d.querySelector('[data-action="industry-submit"]').disabled);c.select('supply','3');assert(c.d.querySelector('[data-action="industry-submit"]').disabled);c.click('industry-recheck');c.click('industry-submit');assert(c.id('receipt').textContent.includes('Result unknown'));c.select('readback','mismatch');c.click('industry-query');assert(c.id('notice').textContent.includes('another job'));assert(c.id('receipt').textContent.includes('Result unknown'));c.select('locale','zh-CN');c.select('locale','en');c.select('readback','settled');c.click('industry-query');assert(c.text().includes('9 iron_ingot'));assert(c.text().includes('Not published in this summary'));assert(!c.text().includes('3 slag'));assert(c.text().includes('not the current inventory balance'));c.close();});
check('Language and copy changes do not turn a current settlement into a historical visit',()=>{const c=context('industry','settled');const before=c.w.__businessSnapshot();c.select('locale','en');c.select('copy-mode','long');assert.equal(c.w.__businessSnapshot(),before);assert(c.text().includes('Original operation result'));assert(!c.text().includes('Return visit · Historical result'));assert(c.id('receipt').textContent.includes('action-demo-17'));c.close();});
check('Missing translation fixture keeps readable source with an explicit source-language annotation',()=>{const c=context('prompt','noop');c.input('prompt-short','Prompt 原文');const before=c.w.__businessSnapshot();c.select('copy-mode','missing');assert.equal(c.id('prompt-source-help').querySelector('[lang="zh-CN"]').textContent,'修改或清除只改变下面的草稿，明确应用后才提交。');assert(c.id('prompt-source-help').textContent.includes('译文缺失'));c.select('locale','en');assert(c.id('prompt-source-help').textContent.includes('Translation unavailable'));assert(c.id('prompt-source-help').textContent.includes('修改或清除'));assert.equal(c.id('prompt-short').value,'Prompt 原文');assert.equal(c.w.__businessSnapshot(),before);c.select('copy-mode','normal');assert(!c.id('prompt-source-help').querySelector('[lang="zh-CN"]'));assert(!c.id('prompt-source-help').textContent.includes('Translation unavailable'));c.close();});
check('Long copy covers fictional names and existing explanations in all three journeys',()=>{const c=context('entry','dense');c.select('locale','en');c.select('copy-mode','long');assert(c.id('world-title').textContent.length>70);assert(c.text().includes('Sample Agent A · Northern Valley'));assert(c.text().includes('Sample Agent C · Northern Valley'));c.id('nav-command').click();assert(c.text().includes('intentionally long fictional description'));c.close();const p=context('prompt');p.select('locale','en');p.select('copy-mode','long');assert(p.id('prompt-source-help').textContent.includes('fictional long explanation'));p.close();const i=context('industry');i.select('locale','en');i.select('copy-mode','long');assert(i.text().includes('fictional long explanation'));assert(i.text().includes('factory-demo-01'));assert(i.text().includes('recipe.smelter.iron_ingot'));i.close();});
check('Review language and copy selections survive fixture and session resets',()=>{const c=context('industry','settled');c.select('locale','en');c.select('copy-mode','missing');c.select('fixture','query-failed');assert.equal(c.id('locale').value,'en');assert.equal(c.id('copy-mode').value,'missing');c.id('account-button').click();c.click('signout');c.click('enter');assert.equal(c.id('locale').value,'en');assert.equal(c.id('copy-mode').value,'missing');assert.equal(c.d.documentElement.lang,'en');assert.equal(c.id('fixture').value,'blocked');assert.equal(c.id('supply').value,'2');c.close();});
check('English accessible names, toolbar options and dialog copy are explicit',()=>{const c=context('prompt','indirect');c.select('locale','en');assert.equal(c.id('locale').getAttribute('aria-label'),'Interface language');assert.equal(c.id('copy-mode').getAttribute('aria-label'),'Copy fixture');assert.equal(c.id('close-drawer').getAttribute('aria-label'),'Close panel');assert.equal(c.id('nav-targets').getAttribute('aria-label'),'Targets');assert.equal(c.id('fixture').selectedOptions[0].textContent,'Full profile · Indirect impact');c.click('prompt-preset');assert.equal(c.id('confirm-title').textContent,'Replace the local draft with a preset?');assert.equal(c.id('cancel-confirm').textContent,'Keep draft');c.id('cancel-confirm').click();assert.equal(c.id('prompt-system').value,'system-D');c.close();});
check('Renderer and private-read boundaries survive English long/missing modes',()=>{const c=context('entry','renderer');c.select('locale','en');c.select('copy-mode','long');assert(c.id('world-art').hidden);assert(!c.id('renderer-message').hidden);assert(c.text().includes('Targets')||c.id('drawer-title').textContent.includes('Targets'));c.close();const p=context('prompt','revoked');p.select('locale','en');p.select('copy-mode','missing');assert.equal(p.id('safe-draft').value,'未提交的本地示例草稿');assert(!p.text().includes('system-A'));assert(!p.text().includes('short-B'));assert(!p.id('prompt-source-help'));assert(!p.d.querySelector('[data-action="prompt-apply"]'));p.close();});
check('Every scenario renders both locales and all copy modes without changing business state',()=>{for(const route of ['entry','prompt','industry']){const seed=context(route);const fixtures=[...seed.id('fixture').options].map(o=>o.value);seed.close();for(const fixture of fixtures){const c=context(route,fixture);const before=c.w.__businessSnapshot();for(const locale of ['en','zh-CN'])for(const mode of ['normal','long','missing']){c.select('locale',locale);c.select('copy-mode',mode);assert.equal(c.w.__businessSnapshot(),before,`${route}/${fixture}/${locale}/${mode}`);assert.equal(c.id('fixture').value,fixture);assert(!/\{(?:agent|version|current|draft|count|required|amount|time|material|field|suffix)\}/.test(c.d.body.textContent));}c.close();}}});
check('English author-owned normal UI has no untranslated Chinese outside language choices and source data',()=>{for(const route of ['entry','prompt','industry']){const seed=context(route);const fixtures=[...seed.id('fixture').options].map(o=>o.value);seed.close();for(const fixture of fixtures){const c=context(route,fixture);c.select('locale','en');if(route==='entry'&&fixture==='return')c.id('nav-command').click();const walker=c.d.createTreeWalker(c.d.body,c.w.NodeFilter.SHOW_TEXT);let n;const untranslated=[];while(n=walker.nextNode()){if(n.parentElement.closest('textarea,#locale'))continue;const value=n.textContent.trim();if(/[\u3400-\u9fff]/.test(value)&&value!=='恢复北侧补给')untranslated.push(value);}assert.deepEqual(untranslated,[],`${route}/${fixture}`);c.close();}}});

check('Restored entry session selects the return fixture and retains the original goal receipt',()=>{const c=context('entry');c.select('locale','en');c.click('enter-return');assert.equal(c.id('fixture').value,'return');assert(c.id('receipt').textContent.includes('chat-demo-01'));assert(c.id('receipt').textContent.includes('Agent A'));c.id('nav-command').click();assert(c.text().includes('恢复北侧补给'));const before=c.w.__businessSnapshot();c.select('copy-mode','long');assert.equal(c.w.__businessSnapshot(),before);c.close();});

const sha256 = (text) => crypto.createHash('sha256').update(text).digest('hex');
const result = {
  kind: 'JSDOM source-level DOM walkthrough; not a real browser or layout/IME/AT acceptance',
  node_version: process.version,
  html_sha256: sha256(html),
  i18n_sha256: sha256(i18n),
  script_sha256: sha256(js),
  checks,
  passed: checks.filter((check) => check.result === 'pass').length,
  total: checks.length,
};
console.log(JSON.stringify(result, null, 2));
process.exitCode = result.passed === result.total ? 0 : 1;
