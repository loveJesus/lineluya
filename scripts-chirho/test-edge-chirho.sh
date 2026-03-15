#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Lineluya Edge Integration Test (C1-015)
# Tests the CF Worker edge deployment end-to-end
# Usage: ./scripts-chirho/test-edge-chirho.sh [worker-url]

set -euo pipefail

WORKER_URL_CHIRHO="${1:-http://localhost:8787}"
PASS_CHIRHO=0
FAIL_CHIRHO=0

echo "=== Lineluya Edge Integration Test ==="
echo "Worker: $WORKER_URL_CHIRHO"
echo "John 3:16"
echo ""

test_endpoint_chirho() {
    local name_chirho="$1"
    local url_chirho="$2"
    local expected_chirho="$3"
    local method_chirho="${4:-GET}"

    local response_chirho
    response_chirho=$(curl -s -o /dev/null -w "%{http_code}" -X "$method_chirho" "$url_chirho" 2>/dev/null || echo "000")

    if [ "$response_chirho" = "$expected_chirho" ]; then
        echo "  [PASS] $name_chirho (HTTP $response_chirho)"
        PASS_CHIRHO=$((PASS_CHIRHO + 1))
    else
        echo "  [FAIL] $name_chirho (expected $expected_chirho, got $response_chirho)"
        FAIL_CHIRHO=$((FAIL_CHIRHO + 1))
    fi
}

echo "--- HTTP Routes ---"
test_endpoint_chirho "Boot page" "$WORKER_URL_CHIRHO/" "200"
test_endpoint_chirho "Health check" "$WORKER_URL_CHIRHO/health-chirho" "200"
test_endpoint_chirho "WASM kernel" "$WORKER_URL_CHIRHO/kernel.wasm" "200"
test_endpoint_chirho "Block device info" "$WORKER_URL_CHIRHO/dev/sda-chirho/info-chirho" "200"
test_endpoint_chirho "KV list" "$WORKER_URL_CHIRHO/proc/kv-chirho" "200"
test_endpoint_chirho "SQLite tables" "$WORKER_URL_CHIRHO/dev/sqlite-chirho/tables-chirho" "200"
test_endpoint_chirho "Kernel state" "$WORKER_URL_CHIRHO/kernel-state-chirho/state-chirho" "200"
test_endpoint_chirho "404 fallback" "$WORKER_URL_CHIRHO/nonexistent" "404"

echo ""
echo "--- Content Checks ---"

# Health check returns JSON with version
health_body_chirho=$(curl -s "$WORKER_URL_CHIRHO/health-chirho" 2>/dev/null || echo "{}")
if echo "$health_body_chirho" | grep -q "alive_chirho" 2>/dev/null; then
    echo "  [PASS] Health returns alive_chirho field"
    PASS_CHIRHO=$((PASS_CHIRHO + 1))
else
    echo "  [FAIL] Health missing alive_chirho field"
    FAIL_CHIRHO=$((FAIL_CHIRHO + 1))
fi

# Boot page contains John 3:16
boot_body_chirho=$(curl -s "$WORKER_URL_CHIRHO/" 2>/dev/null || echo "")
if echo "$boot_body_chirho" | grep -q "John 3:16" 2>/dev/null; then
    echo "  [PASS] Boot page contains John 3:16"
    PASS_CHIRHO=$((PASS_CHIRHO + 1))
else
    echo "  [FAIL] Boot page missing John 3:16"
    FAIL_CHIRHO=$((FAIL_CHIRHO + 1))
fi

echo ""
echo "=== Results: $PASS_CHIRHO passed, $FAIL_CHIRHO failed ==="

if [ "$FAIL_CHIRHO" -gt 0 ]; then
    echo "Some tests failed. Check worker deployment."
    exit 1
else
    echo "All tests passed! Hallelujah!"
    exit 0
fi
