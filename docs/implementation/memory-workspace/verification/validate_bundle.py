#!/usr/bin/env python3
"""Validate this documentation bundle, NOT Vestrace's implementation.
Run: python verification/validate_bundle.py [documentation-root]
Requires Python 3.10+, jsonschema, markdown-it-py. No network requests.
"""
from __future__ import annotations
import ast, hashlib, json, re, sys
from pathlib import Path
from urllib.parse import unquote, urlsplit
from jsonschema import Draft202012Validator, FormatChecker
from markdown_it import MarkdownIt

def object_without_duplicates(pairs):
    out = {}
    for k, v in pairs:
        if k in out:
            raise ValueError(f'duplicate JSON key: {k}')
        out[k] = v
    return out

def load(path: Path):
    return json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=object_without_duplicates)

def expand_documentation_fixture(value: dict) -> dict:
    """Expand the single bounded UTF-8 negative fixture; never execute data."""
    import copy
    if value.get('fixture_format') != 'mw-generated-example/1':
        raise ValueError('unknown generated fixture format')
    if value.get('schema') != 'ImportPreviewRequest':
        raise ValueError('unsupported generated fixture schema')
    operation = value['replace']
    if operation != {'document_index': 0, 'field': 'content', 'unit': 'я', 'repeat': 40000}:
        raise ValueError('unsupported or unbounded fixture operation')
    result = copy.deepcopy(value['base'])
    result['documents'][0]['content'] = 'я' * 40000
    return result

def path_errors(value: str) -> list[str]:
    errors = []
    if not value or value.startswith(('/', '\\')) or '\\' in value or ':' in value or '\0' in value:
        errors.append('unsafe or absolute path')
    if any(p in ('', '.', '..') for p in value.split('/')):
        errors.append('unsafe path component')
    return errors

def semantic_errors(name: str, v: dict) -> list[str]:
    """Illustrative pure checks only. No ACL/storage/transaction guarantees."""
    out = []
    if name == 'ImportPreviewRequest':
        if v.get('mode') == 'documents':
            docs = v.get('documents', [])
            ids, paths, total = set(), set(), 0
            for d in docs:
                if d['external_id'] in ids:
                    out.append('duplicate external identity')
                ids.add(d['external_id'])
                p = d['relative_path']; out.extend(path_errors(p))
                if p.casefold() in paths:
                    out.append('ambiguous path collision')
                paths.add(p.casefold())
                n = len(d['content'].encode('utf-8')); total += n
                if n > 65536:
                    out.append('UTF-8 bytes exceed file limit')
                if d['content'].startswith('\ufeff'):
                    out.append('BOM refused by input contract')
            if total > 8 * 1024 * 1024:
                out.append('decoded batch too large')
        elif v.get('mode') == 'portable':
            out.extend(semantic_errors('PortablePackage', v['package']))
    elif name == 'PortablePackage':
        memory_ids, source_ids, mem_rev_ids, src_rev_ids = set(), set(), set(), set()
        total = 0
        if len(v['memories']) + len(v['sources']) > 100:
            out.append('portable entity limit')
        for m in v['memories']:
            if m['foreign_id'] in memory_ids: out.append('duplicate memory identity')
            memory_ids.add(m['foreign_id'])
            owned_ids, numbers = set(), []
            for r in m['revisions']:
                if r['foreign_id'] in mem_rev_ids: out.append('duplicate memory revision identity')
                mem_rev_ids.add(r['foreign_id']); owned_ids.add(r['foreign_id']); numbers.append(r['number'])
                n = len(r['content'].encode('utf-8')); total += n
                if n > 65536: out.append('portable revision byte limit')
            if numbers != sorted(set(numbers)): out.append('revision order/number duplicate')
            if m['active_foreign_revision_id'] not in owned_ids: out.append('active revision outside memory')
        external_ids=set()
        for s in v['sources']:
            if s['foreign_id'] in source_ids: out.append('duplicate source identity')
            source_ids.add(s['foreign_id']); out.extend(path_errors(s['relative_path']))
            if s['external_id'] in external_ids: out.append('duplicate source external_id')
            external_ids.add(s['external_id']); numbers=[]
            for r in s['revisions']:
                if r['foreign_id'] in src_rev_ids: out.append('duplicate source revision identity')
                src_rev_ids.add(r['foreign_id']); numbers.append(r['number'])
                n=len(r['content'].encode('utf-8')); total += n
                if n > 65536: out.append('portable source byte limit')
            if numbers != sorted(set(numbers)): out.append('source revision order duplicate')
        seen_links=set()
        for l in v['revision_links']:
            pair=(l['memory_foreign_revision_id'],l['source_foreign_revision_id'])
            if pair[0] not in mem_rev_ids or pair[1] not in src_rev_ids: out.append('dangling provenance reference')
            if pair in seen_links: out.append('duplicate provenance link')
            seen_links.add(pair)
        if total > 8*1024*1024: out.append('portable decoded byte limit')
    elif name == 'ContextResponse':
        b=v['budget']; actual=len(v['rendered_context'].encode('utf-8'))
        if actual != b['used_utf8_bytes'] or actual > b['max_utf8_bytes']:
            out.append('rendered byte accounting mismatch')
        if not b['token_budget_enforced'] and (b['tokenizer_id'] is not None or b['used_tokens'] is not None):
            out.append('false tokenizer precision in byte-only result')
    return out

def walk_json(v):
    yield v
    if isinstance(v, dict):
        for x in v.values(): yield from walk_json(x)
    elif isinstance(v,list):
        for x in v: yield from walk_json(x)

def pointer(doc, ref):
    if not ref.startswith('#/'): raise ValueError(f'external ref not checked: {ref}')
    p=doc
    for part in ref[2:].split('/'):
        p=p[part.replace('~1','/').replace('~0','~')]
    return p

def heading_slug(text: str) -> str:
    text = text.lower().strip().replace('`','')
    text = re.sub(r'[^\w\-\s]', '', text, flags=re.UNICODE)
    return re.sub(r'\s','-',text)

def main(root: Path) -> dict:
    root=root.resolve(); errors=[]; metrics={}
    jsons=list(root.rglob('*.json'))
    for p in jsons:
        try: load(p)
        except Exception as e: errors.append(f'{p.relative_to(root)}: JSON: {e}')
    metrics['json_files_parsed']=len(jsons)
    schema=load(root/'contracts/contracts.schema.json'); Draft202012Validator.check_schema(schema)
    defs=schema['$defs']; metrics['schema_definitions']=len(defs)
    api=load(root/'contracts/openapi.proposed.json')
    operation_ids=[]; methods={'get','post','put','delete','patch','head','options','trace'}
    nrefs=0
    for path,pathobj in api['paths'].items():
        for method,op in pathobj.items():
            if method not in methods: continue
            operation_ids.append(op['operationId'])
            expected=set(re.findall(r'\{([^}]+)\}',path))
            observed={x['name'] for x in op.get('parameters',[]) if x['in']=='path' and x.get('required')}
            if expected != observed: errors.append(f'path parameter mismatch {method} {path}')
    if len(operation_ids)!=len(set(operation_ids)): errors.append('duplicate OpenAPI operationId')
    for v in walk_json(api):
        if isinstance(v,dict) and '$ref' in v:
            try: pointer(api,v['$ref']); nrefs+=1
            except Exception as e: errors.append(f'OpenAPI ref: {e}')
    metrics['proposed_http_operations']=len(operation_ids); metrics['openapi_refs_resolved']=nrefs
    catalog=load(root/'examples/catalog.json'); valid=invalid=0; structural=semantic=0
    for ex in catalog:
        inst=load(root/ex['path'])
        if ex.get('generated_fixture'):
            inst=expand_documentation_fixture(inst)
        validator=Draft202012Validator({'$ref':'#/$defs/'+ex['schema'],'$defs':defs},format_checker=FormatChecker())
        se=list(validator.iter_errors(inst))
        me=[] if se else semantic_errors(ex['schema'],inst)
        if ex['expected']=='valid':
            valid+=1
            if se or me: errors.append(f"{ex['path']}: unexpected refusal: {[e.message for e in se]} {me}")
        else:
            invalid+=1
            if ex['rejection_layer']=='schema':
                structural+=1
                if not se: errors.append(f"{ex['path']}: expected structural refusal")
            else:
                semantic+=1
                if se: errors.append(f"{ex['path']}: semantic example failed schema first: {se[0].message}")
                elif not me: errors.append(f"{ex['path']}: expected semantic refusal")
    metrics.update(valid_examples=valid,invalid_examples=invalid,structural_negative_examples=structural,semantic_negative_examples=semantic)
    md=MarkdownIt('commonmark'); mds=list(root.rglob('*.md')); asts={}; headings={}
    for p in mds:
        txt=p.read_text(encoding='utf-8'); ts=md.parse(txt); asts[p]=ts; hs=set(); counts={}
        for i,t in enumerate(ts):
            if t.type=='heading_open':
                slug=heading_slug(ts[i+1].content); c=counts.get(slug,0);counts[slug]=c+1
                hs.add(slug if not c else slug+'-'+str(c))
        headings[p]=hs
        if re.search(r'(?:[?&]token=|private-user-images\.githubusercontent\.com)',txt): errors.append(f'private URL in {p}')
        opened=None
        for ln,line in enumerate(txt.splitlines(),1):
            m=re.match(r'^\s*(`{3,}|~{3,})',line)
            if not m: continue
            delimiter=m.group(1)
            if opened is None: opened=delimiter
            elif delimiter[0]==opened[0] and len(delimiter)>=len(opened): opened=None
        if opened: errors.append(f'unclosed Markdown fence {p}')
    local_links=0
    for p,ts in asts.items():
        for t in ts:
            for ch in t.children or []:
                if ch.type not in ('link_open','image'):continue
                url=ch.attrGet('href' if ch.type=='link_open' else 'src') or ''
                split=urlsplit(url)
                if split.scheme or split.netloc:continue
                target=(p.parent/unquote(split.path)).resolve() if split.path else p
                if not target.exists(): errors.append(f'{p.relative_to(root)} broken link {url}');continue
                local_links+=1
                if split.fragment and target in headings and unquote(split.fragment) not in headings[target]:
                    errors.append(f'{p.relative_to(root)} missing anchor {url}')
    metrics.update(markdown_files=len(mds),internal_links_checked=local_links)
    tr=load(root/'traceability.json'); req={r['id'] for r in tr['requirements']}; tasks={t['id'] for t in tr['tasks']}; case_ids={c['id'] for c in tr['acceptance_cases']}
    for r in tr['requirements']:
        if r['task'] not in tasks: errors.append('unknown task '+r['id'])
        if not r['tests'] or any(x not in case_ids for x in r['tests']): errors.append('missing tests '+r['id'])
        if not (root/r['spec']).exists() or not (root/r['plan']).exists(): errors.append('missing spec/plan '+r['id'])
        plantext=(root/r['plan']).read_text(encoding='utf-8')
        if r['task'] not in plantext: errors.append('task absent from plan '+r['id'])
    for c in tr['acceptance_cases']:
        if c['requirement'] not in req:errors.append('case unknown requirement '+c['id'])
        if '### '+c['id']+'.' not in (root/'09-acceptance.md').read_text(encoding='utf-8'):errors.append('case absent from human catalog '+c['id'])
    metrics.update(requirements=len(req),implementation_tasks=len(tasks),acceptance_cases=len(case_ids),implementation_packages=len({t['package'] for t in tr['tasks']}))
    fplan=load(root/'file-plan.json'); seen=set()
    for f in fplan['files']:
        if f['path'] in seen:errors.append('duplicate file path '+f['path'])
        seen.add(f['path'])
    metrics['file_plan_entries']=len(seen)
    for f in root.rglob('*.py'):
        try:ast.parse(f.read_text(encoding='utf-8'))
        except SyntaxError as e:errors.append(f'Python syntax {f}: {e}')
    manifest=root/'verification/file-manifest.json'; checked=0
    if manifest.exists():
        for item in load(manifest)['files']:
            p=root/item['path']
            if not p.exists():errors.append('manifest missing '+item['path']);continue
            b=p.read_bytes()
            if len(b)!=item['bytes'] or hashlib.sha256(b).hexdigest()!=item['sha256']:errors.append('manifest drift '+item['path'])
            checked+=1
    metrics['manifest_entries_checked']=checked
    result={'status':'pass' if not errors else 'fail','scope':'documentation only; not product qualification','metrics':metrics,'errors':errors,'not_run':['Rust build/test/doctests','PostgreSQL migrations/roles/races','HTTP/MCP runtime','Browser end-to-end','Source import/export application','Full OpenAPI standards-validator conformance','Live external source availability recheck']}
    return result

if __name__=='__main__':
    try:
        root=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).resolve().parents[1]
        result=main(root); print(json.dumps(result,ensure_ascii=False,indent=2));sys.exit(0 if result['status']=='pass' else 1)
    except Exception as e:
        print(json.dumps({'status':'error','error':str(e)},ensure_ascii=False,indent=2));sys.exit(2)
