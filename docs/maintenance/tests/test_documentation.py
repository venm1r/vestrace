"""Regression tests for documentation tooling, not for Vestrace."""
from __future__ import annotations
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

MAINTENANCE=Path(__file__).resolve().parents[1]
ROOT=MAINTENANCE.parents[1]
sys.dont_write_bytecode=True
sys.path.insert(0,str(MAINTENANCE))
from doc_checks import graph_errors, heading_ids, markdown_scan, normative_parity, parse_json
from render_catalogs import render_roadmap, render_acceptance

spec=importlib.util.spec_from_file_location('mw_checks',ROOT/'docs/implementation/memory-workspace/verification/validate_bundle.py')
mw=importlib.util.module_from_spec(spec);spec.loader.exec_module(mw)

class JsonTests(unittest.TestCase):
    def test_duplicate_keys_are_rejected(self):
        with self.assertRaisesRegex(ValueError,'duplicate'):
            parse_json('{"id":1,"id":2}')
    def test_nonfinite_constants_are_rejected(self):
        for value in ['NaN','Infinity','-Infinity']:
            with self.subTest(value=value),self.assertRaises(ValueError):
                parse_json('{"value":'+value+'}')
    def test_valid_unicode_json_is_preserved(self):
        self.assertEqual(parse_json('{"text":"☀"}')['text'],'☀')

class MarkdownTests(unittest.TestCase):
    def scan(self,readme,other=None):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'docs').mkdir();(root/'README.md').write_text(readme,encoding='utf-8')
            if other is not None:(root/'docs/guide.md').write_text(other,encoding='utf-8')
            return markdown_scan(root)
    def test_missing_target_is_detected(self):
        self.assertTrue(any(x['kind']=='missing-target' for x in self.scan('[Go](docs/missing.md)')['issues']))
    def test_missing_anchor_is_detected(self):
        result=self.scan('[Go](docs/guide.md#absent)','# Present\n')
        self.assertTrue(any(x['kind']=='missing-anchor' for x in result['issues']))
    def test_correct_anchor_is_accepted(self):
        self.assertFalse(self.scan('[Go](docs/guide.md#present)','# Present\n')['issues'])
    def test_duplicate_headings_get_suffixes(self):
        self.assertEqual(heading_ids('# Same\n\n# Same\n'),{'same','same-1'})
    def test_explicit_anchor_is_accepted(self):
        self.assertIn('stable-id',heading_ids('<a id="stable-id"></a>\n# Different'))
    def test_escape_is_rejected(self):
        self.assertTrue(any(x['kind']=='link-escape' for x in self.scan('[Go](../escape.md)')['issues']))
    def test_unclosed_fence_is_detected(self):
        self.assertTrue(any(x['kind']=='unclosed-fence' for x in self.scan('```text\nunfinished\n')['issues']))
    def test_invalid_json_fence_is_detected(self):
        self.assertTrue(any(x['kind']=='json-fence' for x in self.scan('```json\n{"a":}\n```\n')['issues']))
    def test_shell_examples_are_parsed_not_executed(self):
        with tempfile.TemporaryDirectory() as tmp:
            marker=Path(tmp)/'must-not-exist'
            result=self.scan('```bash\nprintf hello > '+str(marker)+'\n```\n')
            self.assertFalse(result['issues']);self.assertFalse(marker.exists())
    def test_bad_bash_is_detected(self):
        self.assertTrue(any(x['kind']=='bash-syntax' for x in self.scan('```bash\nif then\n```\n')['issues']))
    def test_external_links_do_not_trigger_network(self):
        self.assertFalse(self.scan('[External](https://invalid.example/no-network)')['issues'])

class NormativeTests(unittest.TestCase):
    def test_editorial_wrapper_does_not_change_parity(self):
        old='# Test\n\nMUST retain `Memory` and INV-001.\n'
        new='# Test\n\n> **English reading edition** `3e05dfbd`\n\nMUST retain `Memory` and INV-001.\n'
        self.assertEqual(normative_parity(old,new),[])
    def test_weakened_modal_is_detected(self):
        self.assertTrue(normative_parity('MUST deny `x`.','MAY deny `x`.'))
    def test_lost_requirement_is_detected(self):
        self.assertTrue(normative_parity('INV-001 MUST hold.','MUST hold.'))
    def test_changed_technical_identifier_is_detected(self):
        self.assertTrue(normative_parity('Read `revision_id`.','Read `memory_id`.'))
    def test_changed_technical_fence_is_detected(self):
        self.assertTrue(normative_parity('```text\nUNKNOWN\n```','```text\nSucceeded\n```'))

class GraphTests(unittest.TestCase):
    def setUp(self):
        self.features=json.loads((ROOT/'docs/roadmap/feature-register.json').read_text(encoding='utf-8'))['features']
    def test_original_graph_is_valid(self):self.assertEqual(graph_errors(self.features),[])
    def test_cycle_is_detected(self):
        data=deepcopy(self.features);data[0]['depends_on']=[data[0]['id']]
        self.assertTrue(any('cycle' in x for x in graph_errors(data)))
    def test_unknown_dependency_is_detected(self):
        data=deepcopy(self.features);data[0]['depends_on']=['F999']
        self.assertTrue(any('unknown' in x for x in graph_errors(data)))
    def test_duplicate_id_is_detected(self):
        data=deepcopy(self.features);data[1]['id']=data[0]['id']
        self.assertTrue(graph_errors(data))
    def test_unearned_pass_is_detected(self):
        data=deepcopy(self.features);data[0]['verification']='PASS'
        self.assertTrue(any('unearned' in x for x in graph_errors(data)))
    def test_required_v1_scope_cannot_be_downgraded(self):
        data=deepcopy(self.features);next(x for x in data if x['priority']=='P3')['release_relationship']='optional'
        self.assertTrue(any('downgraded' in x for x in graph_errors(data)))
    def test_empty_acceptance_is_detected(self):
        data=deepcopy(self.features);data[0]['acceptance']=[]
        self.assertTrue(any('empty' in x for x in graph_errors(data)))
    def test_generated_acceptance_ids_are_stable(self):
        views=render_roadmap({'features':self.features})
        for feature in self.features:
            text='\n'.join(views.values())
            for index in range(1,len(feature['acceptance'])+1):
                self.assertEqual(text.count('**'+feature['id']+f'-AC{index:02d}:**'),1)

class FixtureTests(unittest.TestCase):
    def test_multibyte_negative_fixture_still_exceeds_byte_limit(self):
        path=ROOT/'docs/implementation/memory-workspace/examples/invalid/utf8-byte-limit.json'
        fixture=json.loads(path.read_text(encoding='utf-8'));expanded=mw.expand_documentation_fixture(fixture)
        text=expanded['documents'][0]['content']
        self.assertEqual(len(text),40000);self.assertEqual(len(text.encode('utf-8')),80000)
        self.assertIn('UTF-8 bytes exceed file limit',mw.semantic_errors('ImportPreviewRequest',expanded))
    def test_ascii_substitution_cannot_change_fixture_semantics(self):
        path=ROOT/'docs/implementation/memory-workspace/examples/invalid/utf8-byte-limit.json'
        fixture=json.loads(path.read_text(encoding='utf-8'));fixture['replace']['unit']='a'
        with self.assertRaises(ValueError):mw.expand_documentation_fixture(fixture)
    def test_context_byte_mismatch_is_detected(self):
        value={'budget':{'used_utf8_bytes':1,'max_utf8_bytes':2,'token_budget_enforced':False,'tokenizer_id':None,'used_tokens':None},'rendered_context':'☀'}
        self.assertIn('rendered byte accounting mismatch',mw.semantic_errors('ContextResponse',value))
    def test_human_acceptance_contains_every_mapped_case(self):
        trace=json.loads((ROOT/'docs/implementation/memory-workspace/traceability.json').read_text(encoding='utf-8'))
        text=render_acceptance(trace)
        self.assertEqual(len(trace['acceptance_cases']),54)
        for case in trace['acceptance_cases']:self.assertEqual(text.count('### '+case['id']+'. '),1)

if __name__=='__main__':unittest.main()
