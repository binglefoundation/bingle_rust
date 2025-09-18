from collections.abc import Iterator

import pytest
import algosdk
from algopy_testing import AlgopyTestContext, algopy_testing_context
from _algopy_testing.primitives.uint64 import UInt64

from smart_contracts.bingle_dapp.contract import BingleDapp


def _create_test_asset(context: AlgopyTestContext) -> int:
    """Create a minimal asset in the algopy_testing ledger and return its id."""
    # Allocate a new asset id and seed minimal params required by ledger helpers
    asset_id = context.ledger._get_next_asset_id()  # type: ignore[attr-defined]
    # Seed required asset params; at minimum default_frozen must be present
    context.ledger._asset_data[asset_id] = {  # type: ignore[attr-defined]
        "default_frozen": False,
    }
    return asset_id


@pytest.fixture()
def context() -> Iterator[AlgopyTestContext]:
    with algopy_testing_context() as ctx:
        yield ctx


def test_hello(context: AlgopyTestContext) -> None:
    # Arrange
    dummy_input = context.any.string(length=10)
    contract = BingleDapp()

    # Act
    output = contract.hello(dummy_input)

    # Assert
    assert output == f"Hello, {dummy_input}"


def test_set_bingle_price_sets_global_state(context: AlgopyTestContext) -> None:
    # Arrange: creator is context.default_sender by default when contract is instantiated
    contract = BingleDapp()
    price = 123456

    # Act: call via a transaction group so Txn/Global are populated
    deferred = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[deferred]):
        deferred.submit()

    # Assert: value stored under the configured key
    stored = context.ledger.get_global_state(contract.__app_id__, b"BinglePrice")
    assert stored == price


def test_set_bingle_price_non_creator_rejected(context: AlgopyTestContext) -> None:
    # Arrange: instantiate contract; creator is context.default_sender
    contract = BingleDapp()
    non_creator_addr = algosdk.account.generate_account()[1]
    non_creator = context.ledger.get_account(non_creator_addr)

    # Prepare call but override active txn sender to a different account
    deferred = context.txn.defer_app_call(contract.set_bingle_price, UInt64(999))

    # Override the sender on the prepared transactions to a non-creator account
    for txn in deferred._txns:  # type: ignore[attr-defined]
        txn.fields["sender"] = non_creator

    # Act / Assert: expect assertion failure due to sender != creator
    with pytest.raises(AssertionError):
        with context.txn.create_group(gtxns=[deferred]):
            deferred.submit()


def test_buy_bingle_success(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 7777

    # Set price via admin method
    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    # Create test asset and opt-in buyer
    asset_id = _create_test_asset(context)
    buyer = context.default_sender
    context.ledger.update_asset_holdings(asset_id, buyer, balance=0)
    # Ensure buyer has at least 1 unit to transfer to themselves in the group
    context.ledger.update_asset_holdings(asset_id, buyer, balance=1)

    # Prepare transactions: asset transfer to buyer and payment to app
    asset = context.ledger.get_asset(asset_id)
    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))

    axfer = context.any.txn.asset_transfer(xfer_asset=asset, asset_amount=UInt64(1), asset_receiver=buyer, sender=buyer)
    pay = context.any.txn.payment(receiver=app_addr, amount=UInt64(price))

    # Prepare app call and include foreign asset
    buy = context.txn.defer_app_call(contract.buy_bingle)
    for txn in buy._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    # Act / Assert: should pass
    with context.txn.create_group(gtxns=[axfer, pay, buy]):
        buy.submit()


def test_buy_bingle_missing_payment(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 5555

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    asset_id = _create_test_asset(context)
    buyer = context.default_sender
    context.ledger.update_asset_holdings(asset_id, buyer, balance=1)
    asset = context.ledger.get_asset(asset_id)

    axfer = context.any.txn.asset_transfer(xfer_asset=asset, asset_amount=UInt64(1), asset_receiver=buyer, sender=buyer)

    buy = context.txn.defer_app_call(contract.buy_bingle)
    for txn in buy._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    # Act / Assert: expect failure due to missing payment
    with pytest.raises(AssertionError):
        with context.txn.create_group(gtxns=[axfer, buy]):
            buy.submit()


def test_buy_bingle_missing_axfer(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 4444

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    asset_id = _create_test_asset(context)
    buyer = context.default_sender
    asset = context.ledger.get_asset(asset_id)

    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))
    pay = context.any.txn.payment(receiver=app_addr, amount=UInt64(price))

    buy = context.txn.defer_app_call(contract.buy_bingle)
    for txn in buy._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    # Act / Assert: expect failure due to missing asset transfer
    with pytest.raises(AssertionError):
        with context.txn.create_group(gtxns=[pay, buy]):
            buy.submit()
