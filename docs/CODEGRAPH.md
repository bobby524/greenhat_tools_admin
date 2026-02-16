# CodeGraphContext (Codegraph MCP) — Setup & Usage

Repo: https://github.com/CodeGraphContext/CodeGraphContext

We use CodeGraphContext to index this repo into a code graph so agents can answer questions like:
- "What calls X?"
- "Show me the call chain from A → B"
- "Where is this type used?"

## Option A (recommended): install locally via pipx

```bash
brew install pipx
pipx ensurepath
pipx install codegraphcontext

# in this repo
cd /Users/bobby/Desktop/API_MCP_Gateway
cgc index .

# run MCP server
cgc mcp start
```

## Option B: run in Docker (experimental)

CodeGraphContext is usually used as a stdio MCP server; containerizing stdio MCP depends on your client.
If we need it containerized, we can run it for indexing/queries and keep MCP itself local.

## Notes / gotchas
- The default backend (FalkorDB Lite) is easiest for local dev.
- If you need Neo4j, run it as a separate docker service and point CGC at it.
- Keep `.cgcignore` updated to avoid indexing `target/`, `vendor/`, and build outputs.
