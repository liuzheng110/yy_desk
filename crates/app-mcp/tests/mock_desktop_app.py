"""Desktop-app stand-in with a changing discovery port and explicit test shutdown."""
import json
import pathlib
import socket
import sys
import time

port_file, count_file, stop_file = map(pathlib.Path, sys.argv[1:4])
mode = sys.argv[4]
with count_file.open('a') as f:
    f.write('started\n')
time.sleep(0.25)
with socket.socket() as server:
    server.bind(('127.0.0.1', 0))
    server.listen()
    server.settimeout(0.1)
    port = server.getsockname()[1]
    if mode != 'never-ready':
        port_file.write_text(json.dumps({'port': port}) if mode == 'json' else str(port))
    deadline = time.monotonic() + 30
    while not stop_file.exists() and time.monotonic() < deadline:
        try:
            conn, _ = server.accept()
            conn.close()
        except socket.timeout:
            pass
