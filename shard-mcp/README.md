# Shard Launcher MCP integration

The recommended connection is Shard's built-in local HTTP MCP server:

1. In Shard, open **Settings → MCP server** and enable it.
2. Use **Copy VS Code config** and add the configuration to your VS Code MCP settings.
3. When VS Code prompts for the token, use **Copy token** in Shard and paste it into the prompt.

The endpoint is `http://127.0.0.1:8542/mcp`. It is reachable only from this computer and only
while Shard is open. The token grants control over local game instances and content; do not share
it or commit it to a repository.

The `shard-mcp` Node package remains available as a stdio bridge for MCP clients that do not
support HTTP MCP. For that legacy setup, build the package with `npm ci` and `npm run build`, then
configure the client to run `node` with the absolute path to `dist/index.js`. Set
`SHARD_API_TOKEN` in both the launcher's environment and the bridge's environment. Never commit
the `.env` file.
