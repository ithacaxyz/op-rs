# `hera`

_Hera is a Rust implementation of the [OP Stack][opstack] Rollup node._

## Overview

Hera can be run as either a standalone node or as an [Execution Extension][exex]
on top of an L1 [Reth][reth] node in the same process.

Under the hood, Hera is powered by the [Kona-derive][kona] library which handles
the [derivation pipeline][derivation] of the L2 payloads from L1 transactions.

## Usage

Hera is still under development.

<!-- Links -->

[reth]: https://github.com/paradigmxyz/reth
[kona]: https://github.com/ethereum-optimism/kona
[exex]: https://www.paradigm.xyz/2024/05/reth-exex
[opstack]: https://docs.optimism.io/
[derivation]: https://docs.optimism.io/stack/protocol/derivation-pipeline
