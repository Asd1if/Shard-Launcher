# Shard Launcher MCP

Use the built-in HTTP MCP server for local clients:

1. In Shard, enable **Settings → MCP server** and copy the link.
2. Add a **Web server** connector named `Shard` in a local MCP client such as LLM Studio and paste
   the link.

The link is a credential: keep it private. It works only on the same computer while Shard is open.
Cloud-hosted MCP connectors cannot reach the local endpoint.

The `shard-mcp` Node package remains as a legacy stdio bridge for clients that need it. Build it
with `npm ci` and `npm run build`, then configure the client to run `node` with the absolute path
to `dist/index.js`. The bridge requires `SHARD_API_TOKEN` to match the launcher's token.
