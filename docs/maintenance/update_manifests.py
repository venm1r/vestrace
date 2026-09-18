#!/usr/bin/env python3
"""Refresh CURRENT editorial manifests after an authorized documentation edit.
Never changes historical manifests, product source, or the original snapshot inventory.
Run the validator afterwards; generating hashes does not prove correctness.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import sys
sys.dont_write_bytecode=True
from doc_checks import sha256, repository_files

EXCLUDED_PARTS={'.git','node_modules','target','__pycache__'}

def files(root: Path):
    return sorted(p for p in repository_files(root) if p.is_file())

def write(path: Path, value: dict):
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def refresh(root: Path) -> dict:
    root=root.resolve();maintenance=root/'docs/maintenance';mw=root/'docs/implementation/memory-workspace'
    snapshot=json.loads((maintenance/'snapshot-files.json').read_text(encoding='utf-8'))
    original={x['path']:x for x in snapshot['files']}
    protected={x['path'] for x in json.loads((maintenance/'preservation-manifest.json').read_text(encoding='utf-8'))['files']}
    active=[p.relative_to(root).as_posix() for p in files(root) if p.suffix=='.md'
            and (p==root/'README.md' or p.is_relative_to(root/'docs')) and p.relative_to(root).as_posix() not in protected]
    write(maintenance/'english-pages.json',{'format':'vestrace-english-authored-pages/1','files':active,
          'intentional_exceptions':'Frozen originals and historical verification reports are preserved with complete English reading companions. Unicode fixture payloads remain byte-exact.'})
    # No new hashes in the scope report: that would create circular dependencies.
    changed=[]
    for path in files(root):
        name=path.relative_to(root).as_posix()
        if name!='README.md' and not name.startswith('docs/'):continue
        old=original.get(name)
        if old is None or sha256(path)!=old['sha256']:
            changed.append({'path':name,'action':'modify' if old else 'add','original_sha256':old['sha256'] if old else None})
    removed=[name for name in original if not (root/name).is_file()]
    write(maintenance/'english-scope.json',{'format':'vestrace-documentation-scope/1','baseline':snapshot['baseline'],
          'changed_paths':changed,'removed_paths':sorted(removed)})
    mw_exclusions={'verification/english-file-manifest.json','verification/english-validation-result.json'}
    members=[{'path':p.relative_to(mw).as_posix(),'bytes':p.stat().st_size,'sha256':sha256(p)}
             for p in files(mw) if p.relative_to(mw).as_posix() not in mw_exclusions]
    write(mw/'verification/english-file-manifest.json',{'format':'vestrace-mw-english-payload/1',
          'exclusions':sorted(mw_exclusions),'files':members})
    exclusions={'docs/maintenance/english-payload-manifest.json','docs/maintenance/english-validation-result.json',
                'docs/implementation/memory-workspace/verification/english-validation-result.json'}
    members=[{'path':p.relative_to(root).as_posix(),'bytes':p.stat().st_size,'sha256':sha256(p)} for p in files(root)
             if (p==root/'README.md' or p.is_relative_to(root/'docs')) and p.relative_to(root).as_posix() not in exclusions]
    write(maintenance/'english-payload-manifest.json',{'format':'vestrace-english-payload/1','exclusions':sorted(exclusions),'files':members})
    return {'current_payload_files':len(members),'english_markdown_pages':len(active),'changed_files':len(changed),'removed_files':len(removed)}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',nargs='?',type=Path,default=Path(__file__).resolve().parents[2])
    args=parser.parse_args()
    print(json.dumps(refresh(args.root),indent=2))
