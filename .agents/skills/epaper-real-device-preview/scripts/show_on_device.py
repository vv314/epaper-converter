#!/usr/bin/env python3
"""Upload local preview artifacts to zero2w and display them on the real e-paper device."""

from __future__ import annotations

import argparse
import shlex
import subprocess
import sys
import time
from pathlib import Path

HOST = "zero2w"
REMOTE_DRIVER_PATH = "/home/pi/RPi_Zero_PhotoPainter/7in3_e-Paper_E/python"
TARGET_SIZE = (800, 480)
PACKED_SIZE = TARGET_SIZE[0] * TARGET_SIZE[1] // 2

REMOTE_RENDER_SCRIPT = r"""
import sys
import time
from pathlib import Path
from PIL import Image

sys.path.insert(0, {driver_path!r})
from lib.waveshare_epd import epd7in3e

palette = {{
    (0, 0, 0): 0x0,
    (255, 255, 255): 0x1,
    (255, 0, 0): 0x3,
    (255, 255, 0): 0x2,
    (0, 0, 255): 0x5,
    (0, 255, 0): 0x6,
}}

targets = {targets!r}
hold_seconds = {hold_seconds}

def load_packed(path: str) -> bytes:
    file_path = Path(path)
    if file_path.suffix.lower() == '.packed':
        packed = file_path.read_bytes()
        if len(packed) != {packed_size}:
            raise SystemExit(f'invalid packed size for {{path}}: {{len(packed)}}')
        return packed

    img = Image.open(file_path).convert('RGB')
    if img.size != {target_size!r}:
        raise SystemExit(f'unexpected size for {{path}}: {{img.size}}')

    pixels = list(img.getdata())
    if len(pixels) % 2 != 0:
        raise SystemExit(f'odd pixel count: {{len(pixels)}}')

    packed = bytearray()
    for idx in range(0, len(pixels), 2):
        try:
            left = palette[pixels[idx]]
            right = palette[pixels[idx + 1]]
        except KeyError as exc:
            raise SystemExit(f'non-palette pixel found in {{path}}: {{exc.args[0]}}')
        packed.append((left << 4) | right)
    return bytes(packed)

epd = epd7in3e.EPD()
epd.init()

for label, path in targets:
    packed = load_packed(path)
    epd.display(packed)
    print(f'displayed {{label}}: {{path}} ({{len(packed)}} bytes)')
    if hold_seconds > 0:
        time.sleep(hold_seconds)

epd.sleep()
print('display command finished')
"""


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)


def ssh(command: str) -> None:
    run(["ssh", HOST, command])


def scp(local_path: Path, remote_path: str) -> None:
    run(["scp", str(local_path), f"{HOST}:{remote_path}"])


def remote_path_for(label: str, local_path: Path) -> str:
    suffix = local_path.suffix or ".bin"
    return f"/tmp/epaper_{label}{suffix}"


def prepare_targets(paths: list[Path]) -> list[tuple[str, Path, str]]:
    if len(paths) == 1:
        return [("single", paths[0], remote_path_for("single", paths[0]))]
    if len(paths) == 2:
        return [
            ("A", paths[0], remote_path_for("A", paths[0])),
            ("B", paths[1], remote_path_for("B", paths[1])),
        ]
    raise ValueError("only single display or A/B display is supported")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Display one or two local artifacts on zero2w")
    parser.add_argument("inputs", nargs="+", help="One file for single display, or two files for A/B display")
    parser.add_argument("--hold-seconds", type=int, default=8, help="How long to keep each frame before switching in A/B mode")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if len(args.inputs) not in (1, 2):
        raise SystemExit("please provide one input file, or two files for A/B display")

    local_paths = [Path(item).expanduser().resolve() for item in args.inputs]
    for path in local_paths:
        if not path.is_file():
            raise SystemExit(f'file not found: {path}')

    targets = prepare_targets(local_paths)

    print(f"Uploading {len(targets)} file(s) to {HOST} ...")
    for label, local_path, remote_path in targets:
        print(f"- {label}: {local_path} -> {remote_path}")
        scp(local_path, remote_path)

    remote_targets = [(label, remote_path) for label, _, remote_path in targets]
    remote_script = REMOTE_RENDER_SCRIPT.format(
        driver_path=REMOTE_DRIVER_PATH,
        targets=remote_targets,
        hold_seconds=max(args.hold_seconds, 0),
        packed_size=PACKED_SIZE,
        target_size=TARGET_SIZE,
    )
    ssh_command = f"python3 - <<'PY'\n{remote_script}\nPY"

    mode = "A/B" if len(targets) == 2 else "single"
    print(f"Starting {mode} display on {HOST} ...")
    ssh(ssh_command)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as exc:
        command = " ".join(shlex.quote(part) for part in exc.cmd)
        print(f"command failed ({exc.returncode}): {command}", file=sys.stderr)
        raise SystemExit(exc.returncode)

