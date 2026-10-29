#!/usr/bin/env python3
"""Real PTY integration test. Build first; optional dependency: pip install pyte.
Runs against an isolated temporary profile and never touches user progress.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time

import pyte

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get('ALGOTYPE_BIN', ROOT / 'target/debug/algotype'))


class Session:
    def __init__(self, directory, environment=None):
        self.master, self.slave = pty.openpty()
        self.screen = pyte.Screen(110, 34)
        self.stream = pyte.Stream(self.screen)
        self.raw = bytearray()
        self.resize(110, 34)
        self.before = termios.tcgetattr(self.slave)
        self.process = subprocess.Popen(
            [str(BINARY), '--data-dir', directory],
            stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env={**os.environ, 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'NO_COLOR': '', **(environment or {})},
        )
        self.pump(0.5)

    def resize(self, cols, rows):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', rows, cols, 0, 0))
        self.screen.resize(rows, cols)
        if hasattr(self, 'process'):
            self.process.send_signal(signal.SIGWINCH)
            self.pump(0.2)

    def pump(self, duration=0.2):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            if select.select([self.master], [], [], min(0.03, max(0, end - time.monotonic())))[0]:
                chunk = os.read(self.master, 65536)
                if not chunk:
                    break
                self.raw.extend(chunk)
                self.stream.feed(chunk.decode('utf-8', errors='replace'))

    def send(self, keys, delay=0.2):
        os.write(self.master, keys.encode())
        self.pump(delay)

    @property
    def display(self):
        return '\n'.join(self.screen.display)

    def capture(self, name):
        directory = ROOT / 'target/terminal-captures'
        directory.mkdir(exist_ok=True)
        (directory / f'{name}.txt').write_text(self.display)
        # Optional visual artifact; PIL is not needed to run the test.
        try:
            from PIL import Image, ImageDraw, ImageFont
            font = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf', 17)
            image = Image.new('RGB', (self.screen.columns * 11 + 40, self.screen.lines * 23 + 30), '#14171c')
            draw = ImageDraw.Draw(image)
            for y, row in self.screen.buffer.items():
                for x, cell in row.items():
                    bg = '#' + cell.bg if len(cell.bg) == 6 else '#14171c'
                    fg = '#' + cell.fg if len(cell.fg) == 6 else '#dee1e0'
                    draw.rectangle((20+x*11, 15+y*23, 30+x*11, 37+y*23), fill=fg if cell.reverse else bg)
            for y, row in sorted(self.screen.buffer.items()):
                for x, cell in sorted(row.items()):
                    fg = '#' + cell.fg if len(cell.fg) == 6 else '#dee1e0'
                    bg = '#' + cell.bg if len(cell.bg) == 6 else '#14171c'
                    if cell.reverse:
                        fg, bg = bg, fg
                    px, py = 20 + x * 11, 15 + y * 23
                    if len(cell.data) == 1 and 0x2800 <= ord(cell.data) <= 0x28ff:
                        bits = ord(cell.data) - 0x2800
                        for bit, (dx, dy) in enumerate(((1,2),(1,7),(1,12),(6,2),(6,7),(6,12),(1,17),(6,17))):
                            if bits & (1 << bit):
                                draw.ellipse((px+dx, py+dy, px+dx+2, py+dy+2), fill=fg)
                    else:
                        draw.text((px, py), cell.data, font=font, fill=fg)
                    if cell.underscore:
                        draw.line((px, py + 20, px + 10, py + 20), fill=fg)
            image.save(directory / f'{name}.png')
        except ImportError:
            pass

    def close(self):
        self.send('\x03')
        assert self.process.wait(timeout=5) == 0, self.display
        assert termios.tcgetattr(self.slave) == self.before, 'terminal modes were not restored'
        assert b'\x1b[?1049l' in self.raw, 'alternate screen was not restored'
        os.close(self.master)
        os.close(self.slave)


def main():
    with tempfile.TemporaryDirectory(prefix='algotype-smoke-') as directory:
        session = Session(directory)
        try:
            assert 'a l g o t y p e' in session.display
            session.send('\t\t')  # 25 words
            session.capture('practice')
            target = ' '.join(' '.join(session.screen.display[13:18]).split())
            assert len(target.split()) == 25, (target, session.display)
            session.send('\x1b[200~not real typing\x1b[201~')
            assert 'Paste is paused' in session.display, session.display
            session.send('x\x7f')  # An actual corrected error must survive in accuracy.
            for char in target:
                session.send(char, float(os.environ.get('ALGOTYPE_KEY_DELAY', '0.015')))
            session.pump(1.)
            assert 'words per minute' in session.display, session.display
            session.capture('results')
            data = json.loads((Path(directory) / 'profile.json').read_text())
            assert data['total_sessions'] == 1
            assert data['sessions'][0]['errors'] == 1
            assert data['sessions'][0]['corrections'] == 1
            assert data['sessions'][0]['accuracy'] < 100
            assert data['skills'], 'no adaptive observations persisted'
            session.send('\x1bOP')  # F1 help, then return without double-counting.
            session.send('\r')
            assert 'words per minute' in session.display
            session.send('i')
            assert 'YOUR HANDS HAVE A HISTORY' in session.display
            session.capture('atlas')
            session.send('\r')
            session.send('\x1bOR')  # F3 settings
            assert 'MAKE YOURSELF AT HOME' in session.display
            session.send('tm')
            assert 'Botanical' in session.display and 'Still' in session.display
            session.capture('settings')
            for width, height in ((44, 18), (30, 10), (80, 24), (110, 34)):
                session.resize(width, height)
                assert session.process.poll() is None
            session.send('\r')
            session.close()
            data = json.loads((Path(directory) / 'profile.json').read_text())
            assert data['total_sessions'] == 1, 'help duplicated the completed session'
            assert data['preferences']['theme'] == 1
            assert data['preferences']['reduced_motion']
            summary = subprocess.check_output([str(BINARY), '--data-dir', directory, '--summary'], text=True)
            assert '1 sessions' in summary and 'Latest: 25 words' in summary
            reopened = Session(directory)
            assert '25 words' in reopened.display
            reopened.send('\x1bOR')
            assert 'Still' in reopened.display
            reopened.close()
            print('PASS: real typing + correction, paste rejection, results, learning, save/reopen, help navigation, settings, resize, terminal restoration')
        finally:
            if session.process.poll() is None:
                session.process.kill()
                session.process.wait()


if __name__ == '__main__':
    main()
