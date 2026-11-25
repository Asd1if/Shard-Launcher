# shard-mcp

Shard Launcher has an MCP server built in (Streamable HTTP at `http://127.0.0.1:8542/mcp`).
Clients that support HTTP MCP servers should connect to it directly — see **Settings → MCP server**.

`shard-mcp.mjs` is a dependency-free stdio bridge for clients that can only start local commands.
It forwards every JSON-RPC message to Shard, so all of Shard's tools are available. Node.js 18+.

```json
{
  "mcpServers": {
    "shard": {
      "command": "node",
      "args": ["/absolute/path/to/shard-mcp/shard-mcp.mjs", "http://127.0.0.1:8542/mcp?token=…"]
    }
  }
}
```

The link (with its token) is copied from **Settings → MCP server** in Shard. It is a credential:
keep it private. It works only on the same computer while Shard is open.
