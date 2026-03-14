#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Build the hello world ELF binary for testing on Lineluya kernel
set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"

# Try to find a suitable assembler/linker
# On macOS, we need a cross-linker for x86_64-linux
if command -v x86_64-linux-gnu-as &>/dev/null; then
    AS_CHIRHO=x86_64-linux-gnu-as
    LD_CHIRHO=x86_64-linux-gnu-ld
elif command -v x86_64-elf-as &>/dev/null; then
    AS_CHIRHO=x86_64-elf-as
    LD_CHIRHO=x86_64-elf-ld
else
    echo "Error: Please install x86_64 cross tools:"
    echo "  brew install x86_64-elf-binutils"
    echo ""
    echo "Alternatively, use the Rust-based build:"
    echo "  cargo +nightly build --release"
    exit 1
fi

cd "$SCRIPT_DIR_CHIRHO"
$AS_CHIRHO -o hello_chirho.o hello_chirho.S
$LD_CHIRHO -T link_chirho.ld -o hello_chirho.elf hello_chirho.o --static
echo "Built: hello_chirho.elf"
file hello_chirho.elf
