#!/usr/bin/env python3
"""Validate static content identity and derivatives, not runtime truth or UX."""
from html.parser import HTMLParser
from pathlib import Path
from xml.etree import ElementTree as ET
from datetime import datetime
from email.utils import parsedate_to_datetime
import sys
import re

ROOT = Path(__file__).resolve().parent.parent / 'site'

class Page(HTMLParser):
    def __init__(self, path):
        super().__init__(convert_charrefs=True)
        self.nodes = []
        self.text = []
        self.regions = {}
        self.stack = []
        self.feed(path.read_text(encoding='utf-8'))
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        self.nodes.append((tag, attrs))
        markers = [key for key in attrs if key in {'data-content-body','data-content-correction','data-content-description','data-content-provenance-label'}]
        if 'data-content-summary' in attrs:
            markers.append('summary:' + attrs['data-content-summary'])
        summary = next((marker.removeprefix('summary:') for _, parent_markers in reversed(self.stack) for marker in parent_markers if marker.startswith('summary:')), None)
        if summary and 'data-content-description' in attrs: markers.append('description:' + summary)
        if summary and tag in {'h2','h3'}: markers.append('title:' + summary)
        if tag == 'h1': markers.append('h1')
        if tag not in {'meta','link','img','br','input','hr'}: self.stack.append((tag, markers))
    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1,-1,-1):
            if self.stack[i][0] == tag:
                del self.stack[i:]; break
    def handle_data(self, data):
        self.text.append(data)
        for _,markers in self.stack:
            for marker in markers: self.regions.setdefault(marker,[]).append(data)
    def region(self, key):
        return ' '.join(''.join(self.regions.get(key,[])).split())
    def with_attr(self, key):
        return [(tag, a) for tag, a in self.nodes if key in a]

def check(root=ROOT):
    errors = []
    families = {'devlog': 'devlog', 'observations': 'observation'}
    paired = {}

    def need(condition, message):
        if not condition:
            errors.append(message)

    def date_value(value):
        try:
            return datetime.strptime(value, '%Y-%m-%d')
        except (ValueError, TypeError):
            return None

    for prefix, locale in [('', 'zh-CN'), ('en/', 'en')]:
        home = Page(root / prefix / 'index.html')
        steps = home.with_attr('data-decision-panel')
        need([a['data-decision-panel'] for _, a in steps] == ['0', '1', '2', '3'], f'{prefix}: four static reading steps required')
        need(all('hidden' not in a for _, a in steps), f'{prefix}: steps must remain readable without JS')
        walkthrough = home.with_attr('data-decision-walkthrough')
        need(len(walkthrough) == 1 and walkthrough[0][1].get('data-provenance') == 'runtime_capture', f'{prefix}: evidence type required')
        if len(walkthrough) == 1:
            attrs = walkthrough[0][1]
            for field in ['data-source-build', 'data-runtime-profile', 'data-source-scenario', 'data-source-window']:
                need(attrs.get(field, '').strip().lower() not in {'', 'unknown', 'n/a', 'none'}, f'{prefix}: incomplete case runtime provenance: {field}')
            provenance = home.with_attr('data-evidence-provenance')
            need(len(provenance) == 1 and provenance[0][1].get('data-evidence-provenance') == 'runtime_capture', f'{prefix}: runtime capture disclosure required')
            if len(provenance) == 1:
                need(provenance[0][1].get('data-evidence-source') == attrs.get('data-evidence-source'), f'{prefix}: runtime capture source disclosure mismatch')
            need(bool(re.fullmatch(r'[0-9a-f]{40}', attrs.get('data-source-version', ''))), f'{prefix}: fixed case version required')
            need(bool(re.fullmatch(r'https://github\.com/(?:nil-studio|eng-cc)/oasis7/blob/[0-9a-f]{40}/doc/testing/evidence/[^?#]+', attrs.get('data-evidence-source', ''))), f'{prefix}: immutable case report required')
        articles = []
        identities = set()
        for family, kind in families.items():
            for path in sorted((root / prefix / family).glob('*/index.html')):
                page = Page(path)
                nodes = page.with_attr('data-content-id')
                need(len(nodes) == 1, f'{path}: one canonical content identity required')
                if len(nodes) != 1:
                    continue
                attrs = nodes[0][1]
                identifier = attrs.get('data-content-id', '')
                need(identifier not in identities, f'{path}: duplicate content identity')
                identities.add(identifier)
                need(attrs.get('data-content-locale') == locale, f'{path}: wrong locale')
                need(attrs.get('data-content-kind') == kind, f'{path}: invalid content kind')
                need(attrs.get('data-content-status') in {'published', 'withdrawn'}, f'{path}: draft or unknown status in public route')
                need(identifier == path.parent.name and re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', identifier), f'{path}: invalid slug or id mismatch')
                need(attrs.get('data-translation-of') == identifier, f'{path}: explicit translation identity missing')
                for field in ['data-published-at', 'data-updated-at']:
                    need(date_value(attrs.get(field)) is not None, f'{path}: invalid {field}')
                need(bool(attrs.get('data-source-version')), f'{path}: source version missing')
                if kind == 'observation' and attrs.get('data-content-status') == 'published':
                    provenance = attrs.get('data-provenance')
                    labels = {'concept': ('概念', 'concept'), 'illustrative': ('说明性', 'illustrative'), 'runtime_capture': ('真实运行记录', 'runtime capture')}
                    need(provenance in labels, f'{path}: observation provenance missing or invalid')
                    label = labels.get(provenance, ('missing', 'missing'))[0 if locale == 'zh-CN' else 1]
                    need(label in page.region('data-content-provenance-label').lower(), f'{path}: visible observation provenance label missing')
                    if provenance == 'runtime_capture':
                        version = attrs.get('data-source-version', '')
                        need(bool(re.fullmatch(r'[0-9a-f]{40}', version)), f'{path}: fixed runtime source commit required')
                        for field in ['data-source-build', 'data-runtime-profile', 'data-source-scenario', 'data-source-window']:
                            need(attrs.get(field, '').strip().lower() not in {'', 'unknown', 'n/a', 'none'}, f'{path}: incomplete runtime provenance: {field}')
                        source = attrs.get('data-source-url', '')
                        need(bool(re.match(rf'^https://github\.com/(?:nil-studio|eng-cc)/oasis7/blob/{re.escape(version)}/', source)), f'{path}: immutable trusted runtime source required')
                        need(any(tag == 'a' and a.get('href') == source for tag, a in page.nodes), f'{path}: runtime source link missing')
                articles.append((family, path, attrs, page))
        paired[locale] = {(family, attrs.get('data-content-id')): attrs for family, _, attrs, _ in articles}
        published_ids = {}
        for family in families:
            published = sorted((entry for entry in articles if entry[0] == family and entry[2].get('data-content-status') == 'published'), key=lambda entry: (-(date_value(entry[2].get('data-published-at')) or datetime(1970, 1, 1)).timestamp(), entry[2].get('data-content-id', '')))
            published_ids[family] = [entry[2].get('data-content-id') for entry in published]
            parsed = Page(root / prefix / family / 'index.html')
            actual = [a['data-content-summary'] for _, a in parsed.with_attr('data-content-summary')]
            need(actual == published_ids[family], f'{prefix}{family}: published order or withdrawal mismatch')
        home_ids = published_ids['observations'][:3] + published_ids['devlog'][:3]
        need([a['data-content-summary'] for _, a in home.with_attr('data-content-summary')] == home_ids, f'{prefix}index.html: published order, capacity or withdrawal mismatch')
        items = ET.parse(root / prefix / 'feed.xml').getroot().findall('channel/item')
        need(len(items) == len(articles), f'{prefix}: RSS/detail set mismatch')
        guids = [item.findtext('guid') for item in items]
        need(len(set(guids)) == len(guids), f'{prefix}: duplicate RSS GUID')
        seen = set()
        for family, path, attrs, page in articles:
            identifier = attrs.get('data-content-id')
            url = f'https://oasis7.tech/{prefix}{family}/{path.parent.name}/'
            item = next((item for item in items if item.findtext('guid') == url), None)
            need(item is not None, f'{path}: missing stable GUID')
            if item is None:
                continue
            seen.add(url)
            need(item.findtext('link') == url, f'{path}: feed target mismatch')
            need(item.findtext('title') == page.region('h1'), f'{path}: feed title drift')
            try:
                publication = parsedate_to_datetime(item.findtext('pubDate')).date().isoformat()
            except (ValueError, TypeError):
                publication = None
            need(publication == attrs.get('data-published-at'), f'{path}: publication date drift')
            for surface in [root / prefix / 'index.html', root / prefix / family / 'index.html']:
                parsed = Page(surface)
                present = any(a['data-content-summary'] == identifier for _, a in parsed.with_attr('data-content-summary'))
                expected_ids = home_ids if surface == root / prefix / 'index.html' else published_ids[family]
                need(present == (identifier in expected_ids), f'{surface}: missing published derivative or withdrawn recommendation')
                if present:
                    need(item.findtext('title') == parsed.region('title:' + identifier), f'{surface}: summary title drift')
                    need(item.findtext('description') == parsed.region('description:' + identifier), f'{surface}: summary description drift')
            if attrs.get('data-content-status') == 'withdrawn':
                need(not page.with_attr('data-content-body'), f'{path}: withdrawn body must be removed')
                need(bool(page.region('data-content-correction')), f'{path}: explicit safe correction required')
                need(item.findtext('description') == page.region('data-content-correction'), f'{path}: withdrawn RSS must match correction')
            else:
                need(len(page.with_attr('data-content-body')) == 1 and bool(page.region('data-content-body')), f'{path}: explicit published body required')
                need(item.findtext('description') == page.region('data-content-description'), f'{path}: canonical summary/RSS drift')
        need(len(seen) == len(items), f'{prefix}: unmatched RSS item')
        for _, attrs in home.with_attr('data-locale-en'):
            need('data-locale-zh' in attrs, f'{prefix}: missing locale map')
    zh = paired['zh-CN']
    en = paired['en']
    need(set(zh) == set(en), 'content locale sets differ; untranslated content requires an explicit language fallback design')
    for identity in set(zh) & set(en):
        need(zh[identity].get('data-content-status') == en[identity].get('data-content-status'), f'{identity}: paired publication status differs; withdrawals must remove both locales')
        if identity[0] == 'observations' and zh[identity].get('data-content-status') == 'published':
            need(zh[identity].get('data-provenance') == en[identity].get('data-provenance'), f'{identity}: paired provenance differs')
    return errors

if __name__ == '__main__':
    errors = check()
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        sys.exit(1)
    print('site-discovery-check: OK (paired content families, provenance, derivatives and RSS)')
