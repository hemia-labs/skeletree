# Security

Skeletree parses and indexes source code from your repo into a local SQLite
file and serves it over MCP. If you find a vulnerability (e.g. a crafted file
that crashes the parser, a path traversal in the indexer, or an MCP tool that
leaks data outside the queried repo), please report it privately rather than
opening a public issue.

Report via [GitHub Security Advisories](https://github.com/hemia-labs/skeletree/security/advisories/new)
for this repo. We'll acknowledge within a few days and aim to ship a fix
before public disclosure.

Only the latest released version is supported.
