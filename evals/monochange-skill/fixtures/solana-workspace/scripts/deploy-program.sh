#!/bin/sh
# Stand-in for `pina deploy --cluster <RPC> --program-keypair
# target/deploy/transfer-keypair.json --upgrade-authority <KP> --payer <KP> --yes`,
# which wraps `solana program deploy` against the target cluster (a local
# Surfpool Surfnet in development).
# Records that the real (mutating) command ran, so dry-run contracts can tell
# the two apart.
set -eu
version=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' deploy/mainnet.json | head -1)
echo "upgrade: deploying version ${version} of transfer to mainnet"
date -u +%Y-%m-%dT%H:%M:%SZ > .deploy-ran
