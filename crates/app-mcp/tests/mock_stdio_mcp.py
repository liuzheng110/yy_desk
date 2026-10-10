#!/usr/bin/env python3
"""Mock stdio MCP server: newline-delimited JSON-RPC over stdin/stdout.

Tools: echo(text) -> "echo:{text}"
"""
import json
import sys


def respond(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = msg.get("method", "")
        rid = msg.get("id")
        if method == "initialize":
            respond({
                "jsonrpc": "2.0", "id": rid,
                "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "serverInfo": {"name": "mock-stdio", "version": "1.0"},
                },
            })
        elif method == "notifications/initialized":
            pass
        elif method == "ping":
            respond({"jsonrpc": "2.0", "id": rid, "result": {}})
        elif method == "tools/list":
            respond({
                "jsonrpc": "2.0", "id": rid,
                "result": {
                    "tools": [
                        {
                            "name": "echo",
                            "description": "echo the given text back",
                            "inputSchema": {
                                "type": "object",
                                "properties": {"text": {"type": "string"}},
                                "required": ["text"],
                            },
                        }
                    ]
                },
            })
        elif method == "tools/call":
            args = msg.get("params", {}).get("arguments", {})
            text = args.get("text", "")
            respond({
                "jsonrpc": "2.0", "id": rid,
                "result": {"content": [{"type": "text", "text": "echo:" + str(text)}]},
            })
        else:
            respond({
                "jsonrpc": "2.0", "id": rid,
                "error": {"code": -32601, "message": "method not found: " + method},
            })


if __name__ == "__main__":
    main()
