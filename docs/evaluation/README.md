# Evaluating memory usefulness

**Status:** Proposed methodology and synthetic fixtures—not experiment results.

The [corpus](corpus.json) contains **8 synthetic sources and 12 control questions**. They
are expectations, not facts about Vestrace or results of execution. Every result remains
`NOT_RUN`. Advanced recorded-as-known cases target F201; report unsupported semantics when
the current product cannot answer that question rather than substitute current-state retrieval.

This English edition preserves source/case IDs, scopes, times, supersession links, and expected/
forbidden evidence. Its text bytes and dataset digest differ from the Russian edition. Pin
the actual corpus digest in any comparison; do not reuse earlier measurements as if unchanged.

## Separate retrieval and answer quality

First inspect permitted exact evidence, forbidden disclosure, currency, and warnings. Then
assess whether the answer is grounded and interprets time correctly without inventing facts.
Issued context is distinct from an independently observed final model request.

Compare simple full-text search with the full Vestrace configuration using the same corpus,
permissions, model revision, and budget. Without matching conditions, improvements cannot
be assigned to a specific memory mechanism.

Security cases are deterministic restrictions, not model-judge scores. Content quality may
use labeled expectations and independent review. Match counts alone prove neither source
quality nor safety.

## Record experiments without rewriting history

Record dataset digest, code/config/model/tokenizer identities, hardware, repetitions, safe
observations, and exclusions. Changing the corpus creates a new experiment. For a small sample,
report the numerator/denominator and failures rather than unsupported high-precision accuracy.

A scope leak, silent overwrite, or false certainty after a crash blocks acceptance even when
average helpfulness improves. Choose the relevant P0/P1/P2 fix rather than automatically
adding retrieval channels.

[Knowledge-quality roadmap](../roadmap/p2-knowledge-quality.md) · [Adoption](../roadmap/adoption.md)
