from algopy import ARC4Contract, UInt64, arc4


class Mini(ARC4Contract):
    """
    Minimal Algorand Python (algopy) ARC-4 contract.

    Exposes a single ABI method `fn(uint64)uint64` that returns 2*x + 1.
    This mirrors the behavior used by the Rust integration tests for the
    TEAL samples in tests/dapp_was.
    """

    @arc4.abimethod()
    def fn(self, x: UInt64) -> UInt64:
        return x * UInt64(2) + UInt64(1)
