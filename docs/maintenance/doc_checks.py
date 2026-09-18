"""Pure/document-only checks shared by the validator and its regression tests."""
from __future__ import annotations
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import unquote, urlsplit
from markdown_it import MarkdownIt

PARSER = MarkdownIt('commonmark')
CYRILLIC = re.compile(r'[\u0400-\u04ff]')
MODALS = re.compile(r'\b(?:MUST NOT|MUST|SHOULD NOT|SHOULD|MAY)\b')
INLINE = re.compile(r'(?<!`)`([^`\n]+)`(?!`)')
REQ_ID = re.compile(r'\b[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{2,4}\b')

def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON key: {key}')
        result[key] = value
    return result

def reject_constant(value):
    raise ValueError(f'non-JSON numeric constant: {value}')

def parse_json(text: str):
    return json.loads(text, object_pairs_hook=unique_object, parse_constant=reject_constant)

def load_json(path: Path):
    return parse_json(path.read_text(encoding='utf-8'))

def repository_files(root: Path):
    """Prune build/dependency directories before walking large developer checkouts."""
    excluded={'.git','node_modules','target','__pycache__','.venv','.pytest_cache','.mypy_cache'}
    for directory, dirs, names in os.walk(root, followlinks=False):
        dirs[:]=[name for name in dirs if name not in excluded]
        for name in names:
            if not name.endswith('.pyc'):
                yield Path(directory)/name


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def heading_slug(text: str) -> str:
    # GitHub-style punctuation removal; preserve Unicode letters, underscore and hyphen.
    text = re.sub(r'<[^>]+>', '', text).lower().strip().replace('`', '')
    return re.sub(r'\s', '-', re.sub(r'[^\w\-\s]', '', text, flags=re.UNICODE))

def heading_ids(text: str) -> set[str]:
    tokens = PARSER.parse(text)
    result = set(re.findall(r'<(?:a|span)\s+[^>]*?id=[\'\"]([^\'\"]+)[\'\"]', text))
    counts = Counter()
    for index, token in enumerate(tokens):
        if token.type == 'heading_open':
            base = heading_slug(tokens[index + 1].content)
            count = counts[base]; counts[base] += 1
            result.add(base if count == 0 else f'{base}-{count}')
    return result

def without_editorial_wrapper(text: str) -> str:
    return '\n'.join(line for line in text.splitlines()
                     if not line.startswith('> **English reading edition')
                     and not re.match(r'^<a id="[^\"]+"></a>$', line))

def normative_parity(original: str, english: str) -> list[str]:
    english = without_editorial_wrapper(english)
    problems = []
    for label, pattern in [('normative keywords', MODALS), ('inline technical identifiers', INLINE), ('requirement identifiers', REQ_ID)]:
        before = Counter(pattern.findall(original)); after = Counter(pattern.findall(english))
        if before != after:
            problems.append(f'{label}: missing {dict(before-after)}; added {dict(after-before)}')
    original_fences = [t for t in PARSER.parse(original) if t.type == 'fence']
    english_fences = [t for t in PARSER.parse(english) if t.type == 'fence']
    if len(original_fences) != len(english_fences):
        problems.append('fenced-block count changed')
    else:
        for i, (old, new) in enumerate(zip(original_fences, english_fences), 1):
            if old.info != new.info:
                problems.append(f'code language changed in block {i}')
            # Prose-labelled diagrams/text may be translated. Non-Cyrillic technical fences stay exact.
            if not CYRILLIC.search(old.content) and old.content != new.content:
                problems.append(f'technical fenced block {i} changed')
    return problems

def graph_errors(features: list[dict]) -> list[str]:
    errors = []
    ids = [f['id'] for f in features]
    if len(ids) != len(set(ids)):
        return ['duplicate feature IDs']
    graph = {f['id']: f['depends_on'] for f in features}
    visiting, done = set(), set()
    def visit(node):
        if node in visiting:
            raise ValueError('dependency cycle at ' + node)
        if node in done:
            return
        visiting.add(node)
        for dep in graph[node]:
            if dep not in graph:
                raise ValueError('unknown dependency ' + dep)
            visit(dep)
        visiting.remove(node); done.add(node)
    try:
        for node in ids:
            visit(node)
    except ValueError as exc:
        errors.append(str(exc))
    for f in features:
        if f['priority'] not in {'P0','P1','P2','P3','P4'}:
            errors.append('unknown priority: ' + f['id'])
        if f['decision_status'] != 'PROPOSED' or f['verification'] != 'NOT_RUN_HERE':
            errors.append('unearned feature verdict: ' + f['id'])
        if not f['deliverables'] or not f['acceptance']:
            errors.append('empty feature specification: ' + f['id'])
        if f['priority'] == 'P3' and f['release_relationship'] != 'required-frozen-v1':
            errors.append('downgraded frozen scope: ' + f['id'])
    return errors

def markdown_scan(root: Path) -> dict:
    root = root.resolve()
    paths = ([root/'README.md'] if (root/'README.md').is_file() else []) + sorted((root/'docs').rglob('*.md'))
    documents = {p: p.read_text(encoding='utf-8') for p in paths}
    anchors = {p: heading_ids(text) for p, text in documents.items()}
    issues = []; links = json_fences = bash_fences = 0
    def issue(path, kind, detail):
        issues.append({'path':path.relative_to(root).as_posix(), 'kind':kind, 'detail':detail})
    for path, text in documents.items():
        lines = text.splitlines()
        for token in PARSER.parse(text):
            if token.type == 'fence':
                end = token.map[1] if token.map else 0
                closer = lines[end-1].strip() if end else ''
                if not re.fullmatch(re.escape(token.markup[0])+'{'+str(len(token.markup))+r',}\s*', closer):
                    issue(path, 'unclosed-fence', str(token.map[0]+1))
                language = token.info.strip().split()[0] if token.info.strip() else ''
                if language == 'json':
                    json_fences += 1
                    try: parse_json(token.content)
                    except ValueError as exc: issue(path,'json-fence',str(exc))
                elif language == 'bash':
                    bash_fences += 1
                    proc = subprocess.run(['bash','-n'], input=token.content, text=True,
                                          capture_output=True, timeout=10)
                    if proc.returncode:
                        issue(path,'bash-syntax',proc.stderr.strip())
            for child in token.children or []:
                if child.type not in {'link_open','image'}:
                    continue
                url = child.attrGet('href' if child.type=='link_open' else 'src') or ''
                split = urlsplit(url)
                if split.scheme or split.netloc:
                    continue
                links += 1
                if split.path.startswith('/'):
                    target = (root/unquote(split.path).lstrip('/')).resolve()
                else:
                    target = (path.parent/unquote(split.path)).resolve() if split.path else path
                if not target.is_relative_to(root):
                    issue(path,'link-escape',url); continue
                if not target.exists():
                    issue(path,'missing-target',url); continue
                if split.fragment and target in anchors and unquote(split.fragment) not in anchors[target]:
                    issue(path,'missing-anchor',url)
    unique = {json.dumps(i,sort_keys=True):i for i in issues}
    return {'metrics': {'markdown_files':len(paths),'local_links':links,'json_fences':json_fences,
                        'bash_fences_parse_only':bash_fences}, 'issues':list(unique.values())}

def structure_snapshot(root: Path) -> dict:
    roadmap = load_json(root/'docs/roadmap/feature-register.json')
    mw = root/'docs/implementation/memory-workspace'
    trace = load_json(mw/'traceability.json')
    return {
        'features': [{k:v for k,v in f.items() if k not in {'title','value','reuse','deliverables','acceptance','non_goals'}} |
                     {'deliverable_count':len(f['deliverables']),'acceptance_count':len(f['acceptance'])} for f in roadmap['features']],
        'requirements': [{k:v for k,v in r.items() if k!='requirement'} for r in trace['requirements']],
        'cases': [{'id':c['id'],'requirement':c['requirement']} for c in trace['acceptance_cases']],
        'tasks': [{k:v for k,v in t.items() if k!='title'} for t in trace['tasks']],
        'file_plan': [{k:v for k,v in f.items() if k!='purpose'} for f in load_json(mw/'file-plan.json')['files']],
        'corpus_sources': [{k:v for k,v in s.items() if k!='text'} for s in load_json(root/'docs/evaluation/corpus.json')['sources']],
        'corpus_cases': [{k:v for k,v in c.items() if k not in {'question','expected_behavior'}} for c in load_json(root/'docs/evaluation/corpus.json')['cases']],
        'source_pins': {str(p.relative_to(root)):[{k:v for k,v in s.items() if k!='observation'} for s in load_json(p)['sources']]
                       for p in [root/'docs/maintenance/sources.json',mw/'source-manifest.json']},
    }
