# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life.
# John 3:16

#!/usr/bin/env python3

import base64
import binascii
import pathlib
import re
import sys


SERIAL_LOG_PATH_CHIRHO = pathlib.Path("/tmp/lineluya-serial-chirho.log")
PPM_OUTPUT_PATH_CHIRHO = pathlib.Path(
    "/home/hallelujah/dev-aleluya/personal-aleluya/lineluya/spec-chirho/screenshot-chirho.ppm"
)
PNG_OUTPUT_PATH_CHIRHO = pathlib.Path(
    "/home/hallelujah/dev-aleluya/personal-aleluya/lineluya/spec-chirho/screenshot-chirho.png"
)
BEGIN_MARKER_CHIRHO = "[FB-DUMP-BEGIN]"
END_MARKER_CHIRHO = "[FB-DUMP-END]"
BASE64_ALLOWED_PATTERN_CHIRHO = re.compile(rb"[^A-Za-z0-9+/=]")


def extract_payload_bytes_chirho(serial_text_chirho: str) -> bytes:
    begin_index_chirho = serial_text_chirho.find(BEGIN_MARKER_CHIRHO)
    if begin_index_chirho < 0:
        raise RuntimeError(f"Missing begin marker: {BEGIN_MARKER_CHIRHO}")

    begin_line_end_chirho = serial_text_chirho.find("\n", begin_index_chirho)
    if begin_line_end_chirho < 0:
        raise RuntimeError("Begin marker line is truncated")

    end_index_chirho = serial_text_chirho.find(END_MARKER_CHIRHO, begin_line_end_chirho)
    if end_index_chirho < 0:
        raise RuntimeError(f"Missing end marker: {END_MARKER_CHIRHO}")

    payload_text_chirho = serial_text_chirho[begin_line_end_chirho:end_index_chirho]
    payload_bytes_chirho = payload_text_chirho.encode("ascii", errors="ignore")
    payload_bytes_chirho = BASE64_ALLOWED_PATTERN_CHIRHO.sub(b"", payload_bytes_chirho)
    if not payload_bytes_chirho:
        raise RuntimeError("No base64 payload found between framebuffer dump markers")

    padding_needed_chirho = (-len(payload_bytes_chirho)) % 4
    if padding_needed_chirho:
        payload_bytes_chirho += b"=" * padding_needed_chirho

    return payload_bytes_chirho


def decode_ppm_bytes_chirho(payload_bytes_chirho: bytes) -> bytes:
    try:
        return base64.b64decode(payload_bytes_chirho, validate=False)
    except binascii.Error as error_chirho:
        raise RuntimeError(f"Base64 decode failed: {error_chirho}") from error_chirho


def maybe_convert_png_chirho(ppm_output_path_chirho: pathlib.Path) -> pathlib.Path | None:
    try:
        from PIL import Image
    except ImportError:
        return None

    with Image.open(ppm_output_path_chirho) as image_chirho:
        image_chirho.save(PNG_OUTPUT_PATH_CHIRHO)

    return PNG_OUTPUT_PATH_CHIRHO


def main_chirho() -> int:
    if not SERIAL_LOG_PATH_CHIRHO.exists():
        print(
            f"Serial log not found: {SERIAL_LOG_PATH_CHIRHO}",
            file=sys.stderr,
        )
        return 1

    serial_text_chirho = SERIAL_LOG_PATH_CHIRHO.read_text(errors="ignore")

    try:
        payload_bytes_chirho = extract_payload_bytes_chirho(serial_text_chirho)
        ppm_bytes_chirho = decode_ppm_bytes_chirho(payload_bytes_chirho)
    except RuntimeError as error_chirho:
        print(f"extract_screenshot_chirho: {error_chirho}", file=sys.stderr)
        return 1

    PPM_OUTPUT_PATH_CHIRHO.write_bytes(ppm_bytes_chirho)
    print(f"Wrote PPM: {PPM_OUTPUT_PATH_CHIRHO}")
    print(f"PPM size: {len(ppm_bytes_chirho)} bytes")

    png_output_path_chirho = maybe_convert_png_chirho(PPM_OUTPUT_PATH_CHIRHO)
    if png_output_path_chirho is not None:
        print(f"Wrote PNG: {png_output_path_chirho}")
    else:
        print("Pillow not available; skipped PNG conversion")

    return 0


if __name__ == "__main__":
    raise SystemExit(main_chirho())
