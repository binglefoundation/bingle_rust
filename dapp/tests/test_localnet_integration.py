# Integration test that deploys the Mini contract to a running localnet and calls fn(x).
#
# This test uses only algosdk to avoid tight coupling to algokit_utils versions.
# It will be skipped if algosdk is not installed or if a localnet isn't running
# at the default endpoints (algod: http://localhost:4001, kmd: http://localhost:4002, token: 'a'*64).

from __future__ import annotations

import base64
import json
from pathlib import Path

import pytest

# This module is replaced by the sandbox-based integration test using algorand-python-testing.
# Keeping it in the repo but marking it skipped avoids relying on a manually started localnet.
pytestmark = pytest.mark.skip(reason="Replaced by algorand-python-testing sandbox-based integration test")


def _require_algosdk_or_skip():
    try:
        import algosdk  # noqa: F401
        from algosdk.v2client.algod import AlgodClient  # noqa: F401
        from algosdk.kmd import KMDClient  # noqa: F401
        from algosdk.atomic_transaction_composer import (  # noqa: F401
            AtomicTransactionComposer,
            AccountTransactionSigner,
        )
        from algosdk.abi import Method  # noqa: F401
        from algosdk import transaction as txn_mod  # noqa: F401
        return True
    except Exception as e:  # pragma: no cover - environment dependent
        pytest.skip(f"algosdk not available: {e}")
        return False


def _get_localnet_clients():
    from algosdk.v2client.algod import AlgodClient
    from algosdk.kmd import KMDClient

    token = "a" * 64
    algod = AlgodClient(token, "http://localhost:4001")
    kmd = KMDClient(token, "http://localhost:4002")  # type: ignore[no-untyped-call]

    # Probe connectivity; if this fails, localnet is likely not running
    try:
        algod.suggested_params()
    except Exception as e:  # pragma: no cover - depends on environment
        pytest.skip(f"Localnet not reachable (algod): {e}")

    # Probe KMD connectivity too
    try:
        kmd.versions()
    except Exception as e:  # pragma: no cover - depends on environment
        pytest.skip(f"Localnet not reachable (kmd): {e}")

    return algod, kmd


def _get_default_localnet_account(algod, kmd):
    # Find the default wallet and export a key for signing
    wallets = kmd.list_wallets()
    wallet = next((w for w in wallets if w.get("name") == "unencrypted-default-wallet"), None)
    if wallet is None:
        # Fallback to first wallet if present
        wallet = wallets[0] if wallets else None
    if wallet is None:
        pytest.skip("No KMD wallets available on localnet")

    wallet_id = wallet["id"]
    handle = kmd.init_wallet_handle(wallet_id, "")
    keys = kmd.list_keys(handle)
    if not keys:
        pytest.skip("No keys in KMD wallet to use for signing")

    addr = keys[0]
    private_key = kmd.export_key(handle, "", addr)

    from algosdk.account import address_from_private_key
    from algosdk.atomic_transaction_composer import AccountTransactionSigner

    signer = AccountTransactionSigner(private_key)
    sender = address_from_private_key(private_key)
    assert sender == addr
    return sender, signer


def _deploy_app_from_arc56(algod, sender: str, signer, arc56_path: Path) -> int:
    """Create an app from the ARC-56 byteCode fields."""
    from algosdk import transaction as txn

    spec = json.loads(arc56_path.read_text())
    approval_b64 = spec["byteCode"]["approval"]
    clear_b64 = spec["byteCode"]["clear"]
    approval_prog = base64.b64decode(approval_b64)
    clear_prog = base64.b64decode(clear_b64)

    sp = algod.suggested_params()

    # Note: Using zeroed schemas (no state)
    global_schema = txn.StateSchema(num_uints=0, num_byte_slices=0)
    local_schema = txn.StateSchema(num_uints=0, num_byte_slices=0)

    create_txn = txn.ApplicationCreateTxn(
        sender=sender,
        sp=sp,
        on_complete=txn.OnComplete.NoOpOC,
        approval_program=approval_prog,
        clear_program=clear_prog,
        global_schema=global_schema,
        local_schema=local_schema,
    )

    signed = create_txn.sign(signer.private_key)  # type: ignore[attr-defined]
    txid = algod.send_transaction(signed)

    from algosdk.transaction import wait_for_confirmation

    result = wait_for_confirmation(algod, txid, 10)
    app_id = result.get("application-index") or result.get("application-index", 0)
    if not app_id:
        # Some algosdk versions use "application-index", others may report in inner-txns; be strict here
        raise AssertionError("Failed to create application on localnet")
    return int(app_id)


def _call_fn_via_abi(algod, app_id: int, sender: str, signer, x: int) -> int:
    # Use AtomicTransactionComposer with ABI Method from signature
    from algosdk.atomic_transaction_composer import AtomicTransactionComposer
    from algosdk.abi import Method

    method = Method.from_signature("fn(uint64)uint64")
    sp = algod.suggested_params()

    atc = AtomicTransactionComposer()
    atc.add_method_call(
        app_id=app_id,
        method=method,
        sender=sender,
        sp=sp,
        signer=signer,
        method_args=[x],
    )
    result = atc.execute(algod, 4)
    # Expect one ABI result
    abi_results = getattr(result, "abi_results", None) or getattr(result, "tx_ids", None)
    if abi_results is None or not result.abi_results:
        raise AssertionError("No ABI results returned from method call")
    return int(result.abi_results[0].return_value)


@pytest.mark.integration
def test_localnet_deploy_and_call_fn():
    # Skip early if algosdk not available
    # if not _require_algosdk_or_skip():  # pragma: no cover - guard
    #    return

    # Get localnet algod/kmd or skip
    algod, kmd = _get_localnet_clients()

    # Get a funded account from KMD
    sender, signer = _get_default_localnet_account(algod, kmd)

    # Deploy app from included ARC-56 spec
    arc56 = Path(__file__).resolve().parents[1] / "src" / "Mini.arc56.json"
    assert arc56.exists(), "Mini.arc56.json not found"

    app_id = _deploy_app_from_arc56(algod, sender, signer, arc56)

    # Call fn(5) and assert 11
    result = _call_fn_via_abi(algod, app_id, sender, signer, 5)
    assert result == 11
