#!/usr/bin/env python3
"""Validate the English documentation locally. Never execute Vestrace or examples.
Usage: python docs/maintenance/validate_documentation.py [repository-root]
Requires Python 3.10+, markdown-it-py, jsonschema, Bash. Outputs JSON; no writes/network.
"""
from __future__ import annotations
from collections import Counter
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
sys.dont_write_bytecode = True
from doc_checks import (CYRILLIC, graph_errors, load_json, markdown_scan,
                        normative_parity, sha256, structure_snapshot, repository_files)
from render_catalogs import generated


def validate(root: Path) -> dict:
    root = root.resolve(); maintenance=root/'docs/maintenance'; errors=[]; metrics={}
    scan = markdown_scan(root); metrics.update(scan['metrics'])
    historical = load_json(maintenance/'historical-link-baseline.json')['issues']
    known = {json.dumps(x,sort_keys=True) for x in historical}
    carried = []; new_issues=[]
    for issue in scan['issues']:
        (carried if json.dumps(issue,sort_keys=True) in known else new_issues).append(issue)
    errors += [f'{i["path"]}: {i["kind"]}: {i["detail"]}' for i in new_issues]
    metrics['pre_existing_reference_or_syntax_issues_retained']=len(carried)
    metrics['new_markdown_issues']=len(new_issues)

    json_paths=sorted((root/'docs').rglob('*.json'))
    for path in json_paths:
        try:load_json(path)
        except (ValueError,OSError) as exc:errors.append(f'{path.relative_to(root)}: {exc}')
    metrics['json_files']=len(json_paths)

    structural=load_json(maintenance/'contract-baseline.json')
    actual=structure_snapshot(root)
    for name in structural:
        if actual[name] != structural[name]:errors.append('contract structure changed: '+name)
    metrics['structural_contract_groups']=len(structural)
    roadmap=load_json(root/'docs/roadmap/feature-register.json')
    errors += graph_errors(roadmap['features'])
    metrics.update(roadmap_features=len(roadmap['features']),roadmap_acceptance_criteria=sum(len(f['acceptance']) for f in roadmap['features']),
                   priority_groups=dict(Counter(f['priority'] for f in roadmap['features'])))
    source_ids={s['id'] for s in load_json(maintenance/'sources.json')['sources']}
    for f in roadmap['features']:
        if set(f['source_ids'])-source_ids:errors.append('unknown source in '+f['id'])
    corpus=load_json(root/'docs/evaluation/corpus.json'); source={s['id'] for s in corpus['sources']}
    for case in corpus['cases']:
        if case['result']!='NOT_RUN':errors.append('unearned corpus result '+case['id'])
        if set(case['expected_evidence']+case['forbidden_evidence'])-source:errors.append('unknown corpus evidence '+case['id'])
    metrics.update(synthetic_sources=len(source),synthetic_cases=len(corpus['cases']))

    views=generated(root)
    for name, text in views.items():
        path=root/name
        if not path.is_file() or path.read_text(encoding='utf-8')!=text:errors.append('generated view drift: '+name)
    metrics['generated_views_checked']=len(views)

    original_specs=sorted((root/'docs/specs').glob('vestrace-*.md'))
    for original in original_specs:
        english=original.parent/'en'/original.name
        if not english.exists():errors.append('missing English specification '+original.name);continue
        errors += [original.name+': '+e for e in normative_parity(original.read_text(encoding='utf-8'),english.read_text(encoding='utf-8'))]
    metrics['normative_reading_editions_checked']=len(original_specs)

    active=load_json(maintenance/'english-pages.json')['files']
    for name in active:
        text=(root/name).read_text(encoding='utf-8')
        # Explicit old-heading anchors preserve inbound compatibility, not authored English prose.
        prose=re.sub(r'<a id="[^\"]+"></a>', '', text)
        if CYRILLIC.search(prose):errors.append('non-English prose in active document '+name)
        if re.search(r'(?:[?&]token=|sk-(?:proj-)?[A-Za-z0-9_-]{24,}|vst_[0-9a-fA-F]{24,})',prose):
            errors.append('credential-like content in '+name)
    metrics['english_markdown_pages_checked']=len(active)

    preserved=load_json(maintenance/'preservation-manifest.json')['files']
    for item in preserved:
        path=root/item['path']
        if not path.is_file() or sha256(path)!=item['sha256']:
            errors.append('preserved source changed/missing: '+item['path'])
    metrics['preserved_files_checked']=len(preserved)
    snapshot={x['path'] for x in load_json(maintenance/'snapshot-files.json')['files']}
    for path in repository_files(root):
        if not path.is_file():continue
        name=path.relative_to(root).as_posix()
        if name not in snapshot and name!='README.md' and not name.startswith('docs/'):
            errors.append('non-documentation file added outside supplied baseline: '+name)
    scope=load_json(maintenance/'english-scope.json')
    if scope['removed_paths']:errors.append('unexpected original file removals: '+str(scope['removed_paths']))

    payload=load_json(maintenance/'english-payload-manifest.json')['files']
    for item in payload:
        path=root/item['path']
        if not path.is_file() or path.stat().st_size!=item['bytes'] or sha256(path)!=item['sha256']:
            errors.append('current payload hash drift: '+item['path'])
    metrics['payload_hashes_checked']=len(payload)

    mw=root/'docs/implementation/memory-workspace'
    proc=subprocess.run([sys.executable,'-B',str(mw/'verification/validate_bundle.py'),str(mw)],
                        capture_output=True,text=True,timeout=45)
    try:mw_result=json.loads(proc.stdout)
    except ValueError:
        mw_result={'status':'error','errors':[proc.stderr or proc.stdout]}
    if proc.returncode or mw_result.get('status')!='pass':errors+=['MW: '+x for x in mw_result.get('errors',[mw_result.get('error','unknown failure')])]
    metrics['mw']=mw_result.get('metrics',{})
    return {'status':'PASS' if not errors else 'FAIL','scope':'documentation only; no product execution or qualification',
            'baseline':'3e05dfbdce063aa44a3a9e5a7a84c274597e8188','metrics':metrics,'errors':errors,
            'pre_existing_issues':carried,
            'not_run':['Rust builds/tests/doctests','PostgreSQL migration/role/race tests','HTTP/MCP/worker runtime',
                       'Browser or official-client interoperability','Real provider/model calls','Actual import/sync/export or backup/restore',
                       'Live external URL checks','Full specialized OpenAPI standards validator','Independent human translation/architecture review']}

if __name__=='__main__':
    try:
        root=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).resolve().parents[2]
        result=validate(root);print(json.dumps(result,ensure_ascii=False,indent=2));raise SystemExit(0 if result['status']=='PASS' else 1)
    except (ValueError,OSError,KeyError,TypeError,subprocess.TimeoutExpired) as exc:
        print(json.dumps({'status':'ERROR','scope':'documentation only','error':str(exc)},ensure_ascii=False,indent=2));raise SystemExit(2)
