"""Windows MCP fixture: reports its console and a deliberately long-lived child."""
import ctypes
import json
import os
import subprocess
import sys

descendant = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'])
for line in sys.stdin:
    msg = json.loads(line)
    if 'id' not in msg:
        continue
    if msg['method'] == 'initialize':
        result = {'protocolVersion': '2025-06-18', 'capabilities': {},
                  'serverInfo': {'name': 'windows-process-probe', 'version': '1'}}
    elif msg['method'] == 'tools/list':
        result = {'tools': [{'name': 'inspect', 'inputSchema': {'type': 'object'}}]}
    else:
        result = {'content': [{'type': 'text', 'text': json.dumps({
            'pid': os.getpid(), 'descendantPid': descendant.pid,
            'consoleWindow': ctypes.windll.kernel32.GetConsoleWindow(),
        })}]}
    print(json.dumps({'jsonrpc': '2.0', 'id': msg['id'], 'result': result}), flush=True)
