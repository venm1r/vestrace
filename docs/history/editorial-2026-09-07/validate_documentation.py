#!/usr/bin/env python3
"""Validate documentation, never Vestrace runtime. No network calls or file writes.
Usage: python docs/maintenance/validate_documentation.py [repository-overlay-root]
Python 3.10+; markdown-it-py and jsonschema. Exit 0 only for this document scope.
"""
from __future__ import annotations
import hashlib, importlib.util, json, re, sys
from pathlib import Path
from urllib.parse import urlsplit, unquote
from markdown_it import MarkdownIt

EXPECTED_BASE='07e2977a20b05c5b16953a206a6d68bdbff3a052'
EXPECTED_MW_TREE='1f69acad8676991b2f435501ef3fd8aec742c3b4'

def no_duplicate_keys(pairs):
    value={}
    for key,item in pairs:
        if key in value: raise ValueError('duplicate JSON key: '+key)
        value[key]=item
    return value

def load(path):
    return json.loads(path.read_text(encoding='utf-8'),object_pairs_hook=no_duplicate_keys)

def blob_hash(data):
    return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()

def tree_hash(root):
    entries=[]
    for p in root.iterdir():
        if p.name=='__pycache__':continue
        if p.is_symlink():raise ValueError('unexpected symlink in documentation tree')
        if p.is_dir():mode='40000';sha=tree_hash(p);key=p.name+'/'
        else:mode='100644';sha=blob_hash(p.read_bytes());key=p.name
        entries.append((key,mode,p.name,sha))
    data=b''.join(mode.encode()+b' '+name.encode()+b'\0'+bytes.fromhex(sha) for _,mode,name,sha in sorted(entries))
    return hashlib.sha1(b'tree '+str(len(data)).encode()+b'\0'+data).hexdigest()

def slug(s):
    s=s.lower().strip().replace('`','')
    return re.sub(r'\s','-',re.sub(r'[^\w\-\s]','',s,flags=re.UNICODE))

def validate(root):
    root=root.resolve();errors=[];metrics={};md=MarkdownIt('commonmark')
    mds=([root/'README.md'] if (root/'README.md').exists() else [])+sorted((root/'docs').rglob('*.md'));trees={};anchors={}
    for p in mds:
        text=p.read_text(encoding='utf-8');ts=md.parse(text);trees[p]=ts
        hs=set(re.findall(r'<a\s+id=["\']([^"\']+)["\']\s*>',text));counts={}
        for i,t in enumerate(ts):
            if t.type=='heading_open':
                key=slug(ts[i+1].content);n=counts.get(key,0);counts[key]=n+1
                hs.add(key if n==0 else key+'-'+str(n))
        anchors[p]=hs
        if re.search(r'https?://\S+[?&]token=|vst_[0-9a-fA-F]{24,}',text):errors.append('credential-like content: '+str(p.relative_to(root)))
    links=0;json_fences=0;bash_fences=0
    import subprocess
    for p,ts in trees.items():
        for t in ts:
            if t.type=='fence' and t.info.strip()=='json':
                json_fences+=1
                try:json.loads(t.content,object_pairs_hook=no_duplicate_keys)
                except Exception as e:errors.append(f'{p.relative_to(root)} JSON fence: {e}')
            if t.type=='fence' and t.info.strip()=='bash':
                bash_fences+=1
                proc=subprocess.run(['bash','-n'],input=t.content.encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                if proc.returncode:errors.append(f'{p.relative_to(root)} bash syntax: {proc.stderr.decode()[:180]}')
            for c in t.children or []:
                if c.type not in ('link_open','image'):continue
                u=c.attrGet('href' if c.type=='link_open' else 'src') or '';s=urlsplit(u)
                if s.scheme or s.netloc:continue
                target=(p.parent/unquote(s.path)).resolve() if s.path else p
                if not target.is_relative_to(root):errors.append(f'link escapes overlay: {p.relative_to(root)} → {u}');continue
                links+=1
                if not target.exists():errors.append(f'broken link: {p.relative_to(root)} → {u}');continue
                if s.fragment and target in anchors and unquote(s.fragment) not in anchors[target]:errors.append(f'broken anchor: {p.relative_to(root)} → {u}')
    jsons=list((root/'docs').rglob('*.json'))
    for p in jsons:
        try:load(p)
        except Exception as e:errors.append(f'{p.relative_to(root)} JSON: {e}')
    roadmap=load(root/'docs/roadmap/feature-register.json');features=roadmap['features'];ids=[f['id'] for f in features]
    if len(set(ids))!=len(ids):errors.append('duplicate feature IDs')
    if roadmap['baseline_commit']!=EXPECTED_BASE:errors.append('roadmap baseline mismatch')
    sources={x['id'] for x in load(root/'docs/maintenance/sources.json')['sources']}
    groups={};graph={f['id']:f['depends_on'] for f in features};acyclic=[];visiting=set();done=set()
    def dfs(i):
        if i in visiting:raise ValueError('dependency cycle at '+i)
        if i in done:return
        visiting.add(i)
        for dep in graph[i]:
            if dep not in graph:raise ValueError('unknown dependency '+dep)
            dfs(dep)
        visiting.remove(i);done.add(i);acyclic.append(i)
    try:
        for i in ids:dfs(i)
    except ValueError as e:errors.append(str(e))
    assertions=0
    for f in features:
        groups[f['priority']]=groups.get(f['priority'],0)+1
        if f['priority'] not in {'P0','P1','P2','P3','P4'}:errors.append('unknown priority '+f['id'])
        if f['decision_status']!='PROPOSED' or f['verification']!='NOT_RUN_HERE':errors.append('unearned feature verdict '+f['id'])
        if not f['deliverables'] or not f['acceptance']:errors.append('empty feature specification '+f['id'])
        assertions+=len(f['acceptance'])
        if any(x not in sources for x in f['source_ids']):errors.append('unknown source '+f['id'])
        if f['priority']=='P3' and f['release_relationship']!='required-frozen-v1':errors.append('frozen scope downgraded '+f['id'])
    corpus=load(root/'docs/evaluation/corpus.json');src={x['id'] for x in corpus['sources']}
    case_ids=[]
    for c in corpus['cases']:
        case_ids.append(c['id'])
        if c['result']!='NOT_RUN':errors.append('unearned corpus result '+c['id'])
        if any(x not in src for x in c['expected_evidence']+c['forbidden_evidence']):errors.append('unknown corpus source '+c['id'])
    if len(case_ids)!=len(set(case_ids)):errors.append('duplicate corpus case IDs')
    mw=root/'docs/implementation/memory-workspace';observed=tree_hash(mw)
    if observed!=EXPECTED_MW_TREE:errors.append('preserved MW tree changed: '+observed)
    # Execute its existing document validator read-only and suppress .pyc creation.
    old_flag=sys.dont_write_bytecode;sys.dont_write_bytecode=True
    try:
        spec=importlib.util.spec_from_file_location('preserved_mw_validator',mw/'verification/validate_bundle.py')
        mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
        mw_result=mod.main(mw)
    finally:sys.dont_write_bytecode=old_flag
    if mw_result['status']!='pass':errors.extend('MW: '+x for x in mw_result['errors'])
    manifest=root/'docs/maintenance/payload-manifest.json';checked=0
    if manifest.exists():
        for item in load(manifest).get('files',[]):
            p=root/item['path']
            if not p.is_file():errors.append('manifest missing '+item['path']);continue
            b=p.read_bytes();checked+=1
            if len(b)!=item['bytes'] or hashlib.sha256(b).hexdigest()!=item['sha256']:errors.append('manifest drift '+item['path'])
    scope_path=root/'docs/maintenance/scope-result.json'
    if scope_path.exists():
        scope=load(scope_path)
        for p in scope.get('changed_paths',[]):
            if p!='README.md' and not p.startswith('docs/'):errors.append('non-documentation change '+p)
            if p.startswith(('docs/implementation/memory-workspace/','docs/development-evidence/','docs/adr/','docs/superpowers/')):errors.append('protected documentation change '+p)
            if p.startswith('docs/specs/') and p!='docs/specs/README.md':errors.append('normative body change '+p)
    metrics.update(markdown_files=len(mds),json_files=len(jsons),local_links_and_anchors=links,json_fences=json_fences,bash_fences_parse_only=bash_fences,roadmap_features=len(features),priority_groups=groups,feature_acceptance_criteria=assertions,synthetic_sources=len(src),synthetic_cases=len(case_ids),payload_hashes_checked=checked,unchanged_mw_tree=observed,mw_metrics=mw_result['metrics'])
    return dict(status='PASS' if not errors else 'FAIL',scope='documentation only; no product execution or qualification',baseline=EXPECTED_BASE,metrics=metrics,errors=errors,not_run=['Rust tests/build/doctests','PostgreSQL migrations/roles/races','HTTP/MCP/worker runtime','Browser or official-client interoperability','Real backup/restore or source import/export','All external URL availability','Full OpenAPI standards validator','Independent human architecture review'])

if __name__=='__main__':
    try:
        root=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).resolve().parents[2]
        result=validate(root);print(json.dumps(result,ensure_ascii=False,indent=2));raise SystemExit(0 if result['status']=='PASS' else 1)
    except (OSError,ValueError,KeyError,TypeError) as e:
        print(json.dumps({'status':'ERROR','error':str(e)},ensure_ascii=False));raise SystemExit(2)
