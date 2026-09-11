# Security Policy

## Supported versions

Vestrace is currently under active pre-1.0 development. Security fixes are made against the current `main` branch unless a release explicitly documents a different support policy.

Older commits, development snapshots, and unmaintained forks are not guaranteed to receive security fixes.

## Reporting a vulnerability

Please do **not** open a public issue for a suspected vulnerability.

Preferred reporting path:

1. Open the repository's **Security** tab.
2. Use **Report a vulnerability** / GitHub private vulnerability reporting when that option is available.
3. Include enough information to reproduce and assess the issue without publishing secrets or unrelated private data.

If private vulnerability reporting is not available, contact the repository owner through an established private channel before sharing exploit details publicly.

A useful report includes:

- affected commit, version, or deployment shape;
- affected component or interface;
- prerequisites and threat model;
- minimal reproduction steps or proof of concept;
- expected versus observed security boundary;
- potential impact;
- suggested mitigation, if known.

Do not include real credentials, production tokens, private datasets, or third-party personal information in a report.

## Security-sensitive areas

Reports are particularly useful when they concern:

- authentication or authorization bypass;
- workspace or tenant isolation failures;
- improper PostgreSQL role or row-level-security behavior;
- secret or credential disclosure;
- provenance, authority, or governance bypass;
- unsafe memory/context mutation across trust boundaries;
- path traversal, command injection, SQL injection, SSRF, or related input-handling issues;
- MCP, HTTP, CLI, Console, import/export, or integration behavior that crosses documented trust boundaries;
- dependency vulnerabilities that are exploitable in Vestrace's supported execution paths.

## Coordinated disclosure

Please allow maintainers a reasonable opportunity to reproduce, assess, and address a vulnerability before public disclosure.

Maintainers should acknowledge a valid private report when practical, clarify missing reproduction information, and coordinate disclosure after a fix or mitigation is available. Response time is best-effort while the project remains pre-1.0 and independently maintained.

## Out of scope

The following generally do not require a private security report unless they create a concrete security impact:

- unsupported deployment configurations that contradict documented requirements;
- denial-of-service claims requiring unrealistic resources with no demonstrated impact;
- scanner-only findings without an affected execution path;
- dependency CVEs that are not reachable or exploitable in Vestrace;
- social engineering against project maintainers;
- issues that require already having the same privileges as the claimed impact.

When in doubt, prefer private reporting over public disclosure.