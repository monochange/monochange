#!/bin/bash
# Benchmark all monochange CLI commands and output results as JSON.
# Usage: ./scripts/benchmark-commands.sh [RESULTS_FILE]
#
# This script measures wall-clock time for all built-in CLI commands
# and outputs structured JSON for regression detection.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BINARY="$ROOT_DIR/target/release/mc"
RESULTS_FILE="${1:-/tmp/monochange-benchmark-$(date +%s).json}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Ensure binary exists
if [[ ! -f "$BINARY" ]]; then
	echo -e "${RED}Error: Release binary not found at $BINARY${NC}"
	echo "Run: cargo build --release -p monochange"
	exit 1
fi

# Results storage (using arrays instead of associative arrays)
RESULT_LABELS=()
RESULT_JSONS=()

echo "╔════════════════════════════════════════════════════════════╗"
echo "║          monochange CLI Command Benchmarks                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Function to benchmark a command
benchmark_command() {
	local label="$1"
	local args="$2"
	local runs=50
	local warmup=5

	# Warmup
	for i in $(seq 1 $warmup); do
		$BINARY $args >/dev/null 2>&1 || true
	done

	# Benchmark
	local times=()
	for i in $(seq 1 $runs); do
		local start_ns=$(date +%s%N)
		$BINARY $args >/dev/null 2>&1 || true
		local end_ns=$(date +%s%N)
		local duration_ms=$(((end_ns - start_ns) / 1000000))
		times+=($duration_ms)
	done

	# Calculate statistics
	local total=0
	local min=${times[0]}
	local max=${times[0]}

	for t in "${times[@]}"; do
		total=$((total + t))
		if ((t < min)); then min=$t; fi
		if ((t > max)); then max=$t; fi
	done

	local mean=$((total / runs))

	# Calculate p95
	IFS=$'\n' sorted=($(sort -n <<<"${times[*]}"))
	unset IFS
	local p95_idx=$(((runs * 95) / 100))
	local p95=${sorted[$p95_idx]}

	RESULT_LABELS+=("$label")
	RESULT_JSONS+=("{\"mean_ms\":$mean,\"min_ms\":$min,\"max_ms\":$max,\"p95_ms\":$p95,\"runs\":$runs}")

	# Print result
	if ((mean < 10)); then
		echo -e "  ${GREEN}✓${NC} $label: ${mean}ms (min: ${min}ms, p95: ${p95}ms)"
	elif ((mean < 100)); then
		echo -e "  ${YELLOW}⚠${NC} $label: ${mean}ms (min: ${min}ms, p95: ${p95}ms)"
	else
		echo -e "  ${RED}✗${NC} $label: ${mean}ms (min: ${min}ms, p95: ${p95}ms)"
	fi
}

# Run benchmarks
echo "Running benchmarks..."
echo ""

benchmark_command "version" "--version"
benchmark_command "help" "--help"
benchmark_command "init help" "init --help"
benchmark_command "check help" "check --help"
benchmark_command "step validate help" "step validate --help"

# Output JSON
echo ""
echo "Results saved to: $RESULTS_FILE"

# Build JSON output
{
	echo "{"
	echo "  \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
	echo "  \"binary\": \"$BINARY\","
	echo "  \"git_commit\": \"$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null || echo 'unknown')\","
	echo "  \"git_branch\": \"$(git -C "$ROOT_DIR" branch --show-current 2>/dev/null || echo 'unknown')\","
	echo "  \"results\": {"

	for i in "${!RESULT_LABELS[@]}"; do
		if ((i > 0)); then
			echo ","
		fi
		echo -n "    \"${RESULT_LABELS[$i]}\": ${RESULT_JSONS[$i]}"
	done

	echo ""
	echo "  }"
	echo "}"
} >"$RESULTS_FILE"

echo ""
echo "JSON output:"
cat "$RESULTS_FILE" | python3 -m json.tool 2>/dev/null || cat "$RESULTS_FILE"
