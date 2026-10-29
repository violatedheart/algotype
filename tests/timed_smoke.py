#!/usr/bin/env python3
"""Optional 30-second wall-clock PTY check, plus NO_COLOR cursor fallback."""
import json
from pathlib import Path
import tempfile
import time
from terminal_smoke import Session

with tempfile.TemporaryDirectory(prefix='algotype-timed-') as directory:
    session = Session(directory, {'NO_COLOR': '1'})
    try:
        assert any(c.reverse for row in session.screen.buffer.values() for c in row.values()), 'NO_COLOR cursor invisible'
        assert b'38;2;' not in session.raw
        session.send('a')
        start = time.monotonic()
        # Keep polling the live emulator, including an actual resize while the clock runs.
        session.resize(80, 24)
        while time.monotonic() - start < 30.5:
            session.pump(0.5)
        assert 'words per minute' in session.display, session.display
        profile = json.loads((Path(directory) / 'profile.json').read_text())
        record = profile['sessions'][0]
        assert record['seconds'] == 30.0, record
        assert record['attempts'] == 1, record
        assert len(record['trace']) == 30, record
        session.send('x')  # Results don't accept accidental typing as a new session.
        assert 'words per minute' in session.display
        session.close()
        print('PASS: exact 30-second deadline, idle trace, live resize, frozen result, NO_COLOR inverted cursor')
    finally:
        if session.process.poll() is None:
            session.process.kill()
            session.process.wait()
