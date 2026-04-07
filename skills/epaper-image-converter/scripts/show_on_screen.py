#!/usr/bin/env python3
"""Convert and display an image directly on the 7.3inch e-Paper E screen."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_DIR = SCRIPT_DIR.parent
EPD_PATH = '/home/pi/RPi_Zero_PhotoPainter/7in3_e-Paper_E/python'
PACKED_SIZE = 800 * 480 // 2
POSSIBLE_BINARIES = [
    SCRIPT_DIR / 'epaper_converter',
]


def resolve_binary() -> Path:
    for path in POSSIBLE_BINARIES:
        if path.exists():
            return path
    searched = '\\n'.join(f'  - {path}' for path in POSSIBLE_BINARIES)
    raise FileNotFoundError(f'epaper_converter binary not found. Searched:\\n{searched}')


RUST_BINARY = resolve_binary()


def is_packed_buffer(path: Path) -> bool:
    return path.suffix.lower() == '.packed' and path.exists() and path.stat().st_size == PACKED_SIZE


def convert_to_packed(image_path: str, *, halftone: str, resize_mode: str, gamma: float, benchmark: bool) -> Path:
    source = Path(image_path)
    if is_packed_buffer(source):
        print('Input is already a packed display buffer; skipping conversion.')
        return source

    tmp = tempfile.NamedTemporaryFile(suffix='.packed', delete=False)
    tmp_path = Path(tmp.name)
    tmp.close()

    cmd = [
        str(RUST_BINARY),
        'convert',
        image_path,
        str(tmp_path),
        '-f', 'packed',
        '--dither', halftone,
        '--resize-mode', resize_mode,
        '--gamma', str(gamma),
    ]
    if benchmark:
        cmd.append('--benchmark')

    print(f'Converting image with halftone={halftone}, resize_mode={resize_mode}, gamma={gamma} to packed buffer ...')
    subprocess.run(cmd, check=True)
    return tmp_path


def display_packed_buffer(packed_path: Path, *, clear: bool = False) -> None:
    packed = packed_path.read_bytes()
    if len(packed) != PACKED_SIZE:
        raise ValueError(f'Invalid packed buffer size: {len(packed)} (expected {PACKED_SIZE})')

    sys.path.insert(0, EPD_PATH)
    from lib.waveshare_epd import epd7in3e

    print(f'Loaded packed buffer: {packed_path} ({len(packed)} bytes)')
    epd = epd7in3e.EPD()
    epd.init()
    if clear:
        print('Clearing panel before display ...')
        epd.Clear()
    epd.display(packed)
    epd.sleep()


def display_image(
    image_path: str,
    *,
    halftone: str = 'bayer',
    resize_mode: str = 'contain',
    gamma: float = 1.0,
    benchmark: bool = False,
    clear: bool = False,
) -> None:
    start = time.perf_counter()
    print(f'Loading image: {image_path}')
    temp_path: Path | None = None

    try:
        packed_path = convert_to_packed(
            image_path,
            halftone=halftone,
            resize_mode=resize_mode,
            gamma=gamma,
            benchmark=benchmark,
        )
        if packed_path != Path(image_path):
            temp_path = packed_path

        print('Initializing e-paper display ...')
        display_packed_buffer(packed_path, clear=clear)

        total = time.perf_counter() - start
        print(f'Display completed in {total:.2f}s')
    except ImportError as exc:
        print(f'Failed to import e-paper driver from {EPD_PATH}: {exc}', file=sys.stderr)
        sys.exit(1)
    except subprocess.CalledProcessError as exc:
        print(f'Packed conversion failed with exit code {exc.returncode}', file=sys.stderr)
        sys.exit(exc.returncode or 1)
    except Exception as exc:
        print(f'Display failed: {exc}', file=sys.stderr)
        sys.exit(1)
    finally:
        if temp_path and temp_path.exists():
            temp_path.unlink()


def main() -> None:
    parser = argparse.ArgumentParser(description='Display an image on the 7.3inch e-Paper E screen')
    parser.add_argument('image', help='Path to the image file or .packed buffer')
    parser.add_argument('--dither', choices=['bayer', 'blue-noise', 'yliluoma', 'atkinson'], default='bayer', help='Dither strategy')
    parser.add_argument('--resize-mode', choices=['stretch', 'contain', 'cover'], default='contain', help='Resize strategy during conversion')
    parser.add_argument('--gamma', type=float, default=1.0, help='Optional gamma correction during conversion (default: 1.0)')
    parser.add_argument('--benchmark', action='store_true', help='Print converter benchmark timing')
    parser.add_argument('--clear', action='store_true', help='Clear panel before display (disabled by default)')
    args = parser.parse_args()

    if not os.path.exists(args.image):
        print(f'File not found: {args.image}', file=sys.stderr)
        sys.exit(1)

    display_image(
        args.image,
        halftone=args.dither,
        resize_mode=args.resize_mode,
        gamma=args.gamma,
        benchmark=args.benchmark,
        clear=args.clear,
    )


if __name__ == '__main__':
    main()
