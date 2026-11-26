#!/usr/bin/env python3
"""Isolated negative publishing/parity cases for the static discovery gate."""
import importlib.util
from pathlib import Path
import shutil
import tempfile

spec=importlib.util.spec_from_file_location('discovery_check',Path(__file__).with_name('site-discovery-check.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
files=['index.html','en/index.html','feed.xml','en/feed.xml','observations/index.html','en/observations/index.html','devlog/index.html','en/devlog/index.html','devlog/discovery-first/index.html','en/devlog/discovery-first/index.html']
def copy_fixture(root):
 # Keep publishing negative cases independent of newly published real articles.
 for file in files:
  target=root/file;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(module.ROOT/file,target)
 import re
 for prefix in ["", "en/"]:
  for rel in ["index.html", "devlog/index.html"]:
   p=root/prefix/rel
   p.write_text(re.sub(r'<article[^>]*data-content-summary="([^"]+)".*?</article>', lambda m: m.group() if m.group(1)=="discovery-first" else "", p.read_text(), flags=re.S))
  p=root/prefix/"feed.xml";tree=module.ET.parse(p);channel=tree.getroot().find("channel")
  for item in list(channel.findall("item")):
   if not item.findtext("guid", "").endswith("/devlog/discovery-first/"):channel.remove(item)
  tree.write(p,encoding="unicode")

cases=[
 *[('index.html', field + '=', 'missing-' + field + '=') for field in ['data-source-build', 'data-runtime-profile', 'data-source-scenario', 'data-source-window']],
 ('index.html','data-evidence-provenance="runtime_capture"','data-evidence-provenance="illustrative"'),
 ('index.html','data-decision-panel="3"','data-decision-panel="0"'),
 ('index.html','id="decision-2"','id="decision-2" hidden'),
 ('en/index.html','data-provenance="runtime_capture"','data-provenance="illustrative"'),
 ('index.html','data-evidence-source=','missing-evidence-source='),
 ('index.html','data-source-version="52c136a7b0c628b551ab169594a19b0b55198d0b"','data-source-version="unknown"'),
 ('devlog/discovery-first/index.html','data-content-status="published"','data-content-status="draft"'),
 ('devlog/discovery-first/index.html','data-content-status="published"','data-content-status="withdrawn"'),
 ('devlog/discovery-first/index.html','data-content-id="discovery-first"','data-content-id="different"'),
 ('devlog/discovery-first/index.html','data-source-version=','wrong-source-version='),
 ('devlog/discovery-first/index.html','data-published-at="2026-10-14"','data-published-at="invalid"'),
 ('en/devlog/discovery-first/index.html','data-content-locale="en"','data-content-locale="zh-CN"'),
 ('feed.xml','isPermaLink="true">https://oasis7.tech/devlog/discovery-first/','isPermaLink="true">https://oasis7.tech/devlog/missing/'),
 ('feed.xml','14 Oct 2026','13 Oct 2026'),
 ('index.html','data-content-summary="discovery-first"','data-content-summary="unrelated"'),
]
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)
 copy_fixture(root)
 assert not module.check(root), module.check(root)
 for file,old,new in cases:
  target=root/file;source=target.read_text();assert old in source,(file,old)
  target.write_text(source.replace(old,new,1))
  assert module.check(root),(file,old,new)
  target.write_text(source)
 for field in ['data-source-build', 'data-runtime-profile', 'data-source-scenario', 'data-source-window']:
  target=root/'index.html';source=target.read_text()
  for value in ['', '   ', 'unknown']:
   target.write_text(module.re.sub(field + r'="[^"]*"', field + '="' + value + '"', source, count=1))
   assert any('incomplete case runtime provenance: ' + field in e for e in module.check(root)), (field, value)
  target.write_text(source)
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)
 copy_fixture(root)
 # Withdrawal must not pass merely because homepage/list recommendations vanish.
 detail=root/'devlog/discovery-first/index.html'
 detail.write_text(detail.read_text().replace('data-content-status="published"','data-content-status="withdrawn"'))
 for rel in ['index.html','devlog/index.html']:
  p=root/rel;p.write_text(p.read_text().replace('data-content-summary="discovery-first"','data-content-summary="removed"'))
 errors=module.check(root)
 assert any('withdrawn body' in e for e in errors),errors
 assert any('withdrawn RSS' in e for e in errors),errors
 # A safe same-GUID correction with body and recommendations removed is accepted.
 import re
 for rel in ['index.html','devlog/index.html']:
  p=root/rel;p.write_text(re.sub(r'<article[^>]*data-content-summary="removed".*?</article>','',p.read_text(),flags=re.S))
 p=detail;s=p.read_text();s=re.sub(r'<article data-content-body>.*?</article>','<p data-content-correction>Record withdrawn; read the current project statement.</p>',s,flags=re.S);p.write_text(s)
 feed=root/'feed.xml';tree=module.ET.parse(feed);tree.find('channel/item/description').text='Record withdrawn; read the current project statement.';tree.write(feed,encoding='unicode')
 assert any('paired publication status differs' in e for e in module.check(root)),module.check(root)
 # Same-GUID correction must be applied to the translated copy as well.
 p=root/'en/devlog/discovery-first/index.html';s=p.read_text().replace('data-content-status="published"','data-content-status="withdrawn"')
 s=re.sub(r'<article data-content-body>.*?</article>','<p data-content-correction>Record withdrawn; read the current project statement.</p>',s,flags=re.S);p.write_text(s)
 for rel in ['en/index.html','en/devlog/index.html']:
  p=root/rel;p.write_text(re.sub(r'<article[^>]*data-content-summary="discovery-first".*?</article>','',p.read_text(),flags=re.S))
 feed=root/'en/feed.xml';tree=module.ET.parse(feed);tree.find('channel/item/description').text='Record withdrawn; read the current project statement.';tree.write(feed,encoding='unicode')
 assert not module.check(root),module.check(root)
 # Canonical description and locale set drift cannot be published silently.
 p=root/'en/devlog/discovery-first/index.html';p.write_text((module.ROOT/'en/devlog/discovery-first/index.html').read_text().replace('A clearer role, an honest example, and a choice to keep reading.','Different summary.'))
 assert any('canonical summary' in e for e in module.check(root))
 p.unlink();assert any('locale sets differ' in e for e in module.check(root))
# Two legitimate same-date items must be checked per record, sorted by slug.
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)
 copy_fixture(root)
 for prefix in ['', 'en/']:
  source=root/prefix/'devlog/discovery-first/index.html'
  target=root/prefix/'devlog/second-note/index.html';target.parent.mkdir(parents=True)
  target.write_text(source.read_text().replace('discovery-first','second-note'))
  for rel in ['index.html','devlog/index.html']:
   p=root/prefix/rel;s=p.read_text()
   match=re.search(r'<article[^>]*data-content-summary="discovery-first".*?</article>',s,re.S);assert match
   s=s[:match.end()]+match.group().replace('discovery-first','second-note')+s[match.end():];p.write_text(s)
  feed=root/prefix/'feed.xml';tree=module.ET.parse(feed);channel=tree.getroot().find('channel')
  import copy
  item=copy.deepcopy(channel.find('item'))
  for field in ['link','guid']:item.find(field).text=item.find(field).text.replace('discovery-first','second-note')
  channel.append(item);tree.write(feed,encoding='unicode')
 assert not module.check(root),module.check(root)
 p=root/'index.html';s=p.read_text();rows=re.findall(r'<article[^>]*data-content-summary="(?:discovery-first|second-note)".*?</article>',s,re.S);assert len(rows)==2
 p.write_text(s.replace(rows[0]+rows[1],rows[1]+rows[0]));assert any('published order' in e for e in module.check(root))
# Zero public entries is an honest valid state.
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)
 copy_fixture(root)
 for prefix in ['', 'en/']:
  shutil.rmtree(root/prefix/'devlog/discovery-first')
  for rel in ['index.html','devlog/index.html']:
   p=root/prefix/rel;p.write_text(re.sub(r'<article[^>]*data-content-summary="discovery-first".*?</article>','',p.read_text(),flags=re.S))
  feed=root/prefix/'feed.xml';tree=module.ET.parse(feed);channel=tree.getroot().find('channel');channel.remove(channel.find('item'));tree.write(feed,encoding='unicode')
 assert not module.check(root),module.check(root)
# Observations share the identity/derivative contract and add provenance checks.
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)
 copy_fixture(root)
 for prefix in ['', 'en/']:
  source=root/prefix/'devlog/discovery-first/index.html';p=root/prefix/'observations/energy-example/index.html';p.parent.mkdir(parents=True)
  label='illustrative example' if prefix else '说明性示例'
  text=source.read_text().replace('discovery-first','energy-example').replace('data-content-kind="devlog"','data-content-kind="observation"').replace('data-content-status="published"','data-content-status="published" data-provenance="illustrative"').replace('<article data-content-body>','<p data-content-provenance-label>'+label+'</p><article data-content-body>')
  p.write_text(text)
  home=root/prefix/'index.html';row=re.search(r'<article[^>]*data-content-summary="discovery-first".*?</article>',home.read_text(),re.S).group().replace('discovery-first','energy-example').replace('devlog/energy-example/','observations/energy-example/')
  home_text=home.read_text();at=home_text.index('<section id="progress"');home.write_text(home_text[:at]+row+home_text[at:])
  listing=root/prefix/'observations/index.html';listing.write_text(listing.read_text().replace('</main>',row+'</main>'))
  feed=root/prefix/'feed.xml';tree=module.ET.parse(feed);channel=tree.getroot().find('channel');item=copy.deepcopy(channel.find('item'))
  for field in ['guid','link']:item.find(field).text=item.find(field).text.replace('devlog/discovery-first','observations/energy-example')
  channel.insert(0,item);tree.write(feed,encoding='unicode')
 assert not module.check(root),module.check(root)
 p=root/'observations/energy-example/index.html';source=p.read_text()
 for old,new in [('data-provenance="illustrative"','data-provenance="runtime_capture"'),('data-content-status="published"','data-content-status="draft"'),('data-content-id="energy-example"','data-content-id="other"'),('data-provenance="illustrative"','data-provenance="live"')]:
  p.write_text(source.replace(old,new));assert module.check(root),(old,new)
 p.write_text(source)
 # A structurally complete runtime fixture is not a claim about real material.
 for prefix in ['', 'en/']:
  p=root/prefix/'observations/energy-example/index.html';s=p.read_text().replace('data-provenance="illustrative"','data-provenance="runtime_capture" data-source-build="fixture-build" data-runtime-profile="fixture" data-source-scenario="fixture-energy" data-source-window="fixture-window" data-source-url="https://github.com/nil-studio/oasis7/blob/b216610f117d044c20ff9aad2d73485c7c9f3174/README.md"')
  s=s.replace('data-content-provenance-label>'+('illustrative example' if prefix else '说明性示例'),'data-content-provenance-label>'+('runtime capture' if prefix else '真实运行记录'))
  s=s.replace('</article>','<a href="https://github.com/nil-studio/oasis7/blob/b216610f117d044c20ff9aad2d73485c7c9f3174/README.md">Fixture source</a></article>',1);p.write_text(s)
 assert not module.check(root),module.check(root)
 p=root/'observations/energy-example/index.html';s=p.read_text();p.write_text(s.replace('data-runtime-profile="fixture"','data-runtime-profile="unknown"'));assert any('incomplete runtime provenance' in e for e in module.check(root))
print('site-discovery-check.test: OK (paired families, zero/one/multiple, runtime fields and withdrawals)')
