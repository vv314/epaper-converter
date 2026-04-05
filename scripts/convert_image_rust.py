#!/usr/bin/env python3
"""
Rust-powered image converter for Waveshare 7.3inch e-Paper E display.
Drop-in replacement for convert_image.py with ~10-100x performance improvement.

Supported colors: Black, White, Red, Yellow, Blue, Green
Resolution: 800 x 480 pixels
"""

import sys
import os
import argparse
import subprocess
import time
from pathlib import Path

# Path to Rust binary - check multiple possible locations
POSSIBLE_BINARIES = [
    Path(__file__).parent.parent / "rust" / "epaper_converter",  # New compiled binary
    Path(__file__).parent.parent / "rust" / "target" / "release" / "epaper_converter",
    Path(__file__).parent.parent / "rust" / "epaper-converter",  # Old binary
    Path(__file__).parent.parent / "rust" / "target" / "release" / "epaper-converter",
]

RUST_BINARY = None
for path in POSSIBLE_BINARIES:
    if path.exists():
        RUST_BINARY = path
        break

def ensure_binary():
    """Ensure Rust binary exists."""
    global RUST_BINARY
    if RUST_BINARY is None or not RUST_BINARY.exists():
        for path in POSSIBLE_BINARIES:
            if path.exists():
                RUST_BINARY = path
                return

        # If no binary found, report error
        print("Rust binary not found!", file=sys.stderr)
        print(f"Searched in: {[str(p) for p in POSSIBLE_BINARIES]}", file=sys.stderr)
        sys.exit(1)

def infer_format(output_path: str) -> str:
    suffix = Path(output_path).suffix.lower()
    return {
        '.bin': 'bin',
        '.packed': 'packed',
        '.png': 'png',
        '.bmp': 'bmp',
    }.get(suffix, 'bmp')

def convert_image(input_path: str, output_path: str, width: int = 800, height: int = 480, halftone: str = 'auto'):
    """
    Convert an image to e-paper compatible format (Rust optimized).

    Args:
        input_path: Path to input image
        output_path: Path to save converted image
        width: Target width (default 800)
        height: Target height (default 480)
        halftone: Halftone algorithm (`bayer`, `blue-noise`, `atkinson`, `auto`)
    """
    ensure_binary()

    cmd = [
        str(RUST_BINARY),
        "convert",
        input_path,
        output_path,
        "-f", infer_format(output_path),
    ]

    cmd.extend(["--halftone", halftone])

    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode != 0:
        raise RuntimeError(f"Conversion failed: {result.stderr}")

    if result.stdout:
        print(result.stdout, end='')

def create_display_buffer(image_path: str, width: int = 800, height: int = 480) -> bytes:
    """
    Create a display buffer suitable for the e-paper driver.
    Returns bytes with color indices (0-5).
    """
    import tempfile
    from PIL import Image
    import numpy as np

    with tempfile.NamedTemporaryFile(suffix='.png', delete=False) as tmp:
        tmp_path = tmp.name

    try:
        # Convert to e-paper format
        convert_image(image_path, tmp_path, width, height, halftone='auto')

        # Load and convert to buffer
        img = Image.open(tmp_path).convert('RGB')

        # Color palette mapping
        palette = {
            (0, 0, 0): 0,       # Black
            (255, 255, 255): 1, # White
            (255, 0, 0): 2,     # Red
            (255, 255, 0): 3,   # Yellow
            (0, 0, 255): 4,     # Blue
            (0, 255, 0): 5,     # Green
        }

        arr = np.array(img)
        buffer = bytearray(width * height)

        for y in range(height):
            for x in range(width):
                rgb = tuple(arr[y, x])
                buffer[y * width + x] = palette.get(rgb, 1)

        return bytes(buffer)
    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)

def main():
    parser = argparse.ArgumentParser(
        description='Convert images for Waveshare 7.3inch e-Paper E display (Rust optimized)'
    )
    parser.add_argument('input', help='Input image path')
    parser.add_argument('output', help='Output image path')
    parser.add_argument(
        '--width', type=int, default=800,
        help='Target width (default: 800)'
    )
    parser.add_argument(
        '--height', type=int, default=480,
        help='Target height (default: 480)'
    )
    parser.add_argument(
        '--halftone', choices=['bayer', 'blue-noise', 'atkinson', 'auto'], default='auto',
        help='Halftone algorithm (default: auto)'
    )
    parser.add_argument(
        '--buffer', action='store_true',
        help='Save raw buffer file (.bin)'
    )
    parser.add_argument(
        '--format', choices=['bmp', 'bin', 'packed', 'png', 'both'],
        help='Output format (default: infer from output extension)'
    )
    parser.add_argument(
        '--benchmark', action='store_true',
        help='Show processing time'
    )

    args = parser.parse_args()

    print(f"Converting {args.input}...")
    print(f"Target size: {args.width}x{args.height}")
    print(f"Halftone: {args.halftone}")
    print(f"Engine: Rust (high-performance)")

    start = time.perf_counter()

    try:
        ensure_binary()

        cmd = [
            str(RUST_BINARY),
            "convert",
            args.input,
            args.output,
            "-w", str(args.width),
            "-H", str(args.height),
            "-f", args.format or infer_format(args.output),
        ]

        cmd.extend(["--halftone", args.halftone])

        result = subprocess.run(cmd)

        elapsed = time.perf_counter() - start
        if args.benchmark:
            print(f"\nProcessing time: {elapsed:.3f}s")

        if result.returncode == 0:
            print(f"Conversion complete: {args.output}")

        sys.exit(result.returncode)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
