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


def test_sell_bingle_success(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 3333

    # Set price
    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    # Create asset and set seller balances
    asset_id = _create_test_asset(context)
    asset = context.ledger.get_asset(asset_id)
    seller = context.default_sender
    # Ensure seller "holds" amount
    amount = UInt64(5)
    context.ledger.update_asset_holdings(asset_id, seller, balance=int(amount))

    # Prepare axfer from seller to app and payment to seller (any payer allowed)
    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))
    axfer = context.any.txn.asset_transfer(
        xfer_asset=asset, asset_amount=amount, asset_receiver=app_addr, sender=seller
    )
    payout = UInt64(price) * amount
    pay = context.any.txn.payment(receiver=seller, amount=payout)

    # Prepare app call and include foreign asset
    sell = context.txn.defer_app_call(contract.sell_bingle, amount)
    for txn in sell._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    # Act / Assert
    with context.txn.create_group(gtxns=[axfer, pay, sell]):
        sell.submit()


def test_sell_bingle_missing_payment(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 2222

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    asset_id = _create_test_asset(context)
    asset = context.ledger.get_asset(asset_id)
    seller = context.default_sender
    amount = UInt64(3)
    context.ledger.update_asset_holdings(asset_id, seller, balance=int(amount))

    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))
    axfer = context.any.txn.asset_transfer(
        xfer_asset=asset, asset_amount=amount, asset_receiver=app_addr, sender=seller
    )

    sell = context.txn.defer_app_call(contract.sell_bingle, amount)
    for txn in sell._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    with pytest.raises(AssertionError):
        with context.txn.create_group(gtxns=[axfer, sell]):
            sell.submit()


def test_sell_bingle_missing_axfer(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 1111

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    seller = context.default_sender
    amount = UInt64(2)

    payout = UInt64(price) * amount
    pay = context.any.txn.payment(receiver=seller, amount=payout)

    sell = context.txn.defer_app_call(contract.sell_bingle, amount)
    # No foreign asset supplied and no axfer

    with pytest.raises(Exception):
        with context.txn.create_group(gtxns=[pay, sell]):
            sell.submit()


def test_register_success(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 2468

    # Set price
    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    # Create asset and fund sender with >= price units
    asset_id = _create_test_asset(context)
    asset = context.ledger.get_asset(asset_id)
    sender = context.default_sender
    context.ledger.update_asset_holdings(asset_id, sender, balance=price)

    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))

    # ASA fee transfer equal to price
    fee_axfer = context.any.txn.asset_transfer(
        xfer_asset=asset, asset_amount=UInt64(price), asset_receiver=app_addr, sender=sender
    )

    # Prepare app call and include foreign asset; pass a handle
    handle = context.any.string(length=5)
    register = context.txn.defer_app_call(contract.register, handle)
    for txn in register._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    with context.txn.create_group(gtxns=[fee_axfer, register]):
        register.submit()

    # Assert local state set
    handle_key = b"Handle"
    time_key = b"HandleTime"
    stored_handle = context.ledger.get_local_state(contract.__app_id__, sender, handle_key)
    stored_time = context.ledger.get_local_state(contract.__app_id__, sender, time_key)
    assert stored_handle.decode() == str(handle)
    assert isinstance(stored_time, int) and stored_time > 0


def test_register_missing_fee(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 1357

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    asset_id = _create_test_asset(context)
    asset = context.ledger.get_asset(asset_id)

    handle = context.any.string(length=3)
    register = context.txn.defer_app_call(contract.register, handle)
    for txn in register._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)

    with pytest.raises(AssertionError):
        with context.txn.create_group(gtxns=[register]):
            register.submit()


def test_register_does_not_overwrite_existing(context: AlgopyTestContext) -> None:
    # Arrange
    contract = BingleDapp()
    price = 2468

    set_price = context.txn.defer_app_call(contract.set_bingle_price, UInt64(price))
    with context.txn.create_group(gtxns=[set_price]):
        set_price.submit()

    asset_id = _create_test_asset(context)
    asset = context.ledger.get_asset(asset_id)
    sender = context.default_sender
    context.ledger.update_asset_holdings(asset_id, sender, balance=price * 2)
    app_addr = context.ledger.get_account(algosdk.logic.get_application_address(contract.__app_id__))

    # First registration
    fee1 = context.any.txn.asset_transfer(xfer_asset=asset, asset_amount=UInt64(price), asset_receiver=app_addr, sender=sender)
    first_handle = context.any.string(length=5)
    reg1 = context.txn.defer_app_call(contract.register, first_handle)
    for txn in reg1._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)
    with context.txn.create_group(gtxns=[fee1, reg1]):
        reg1.submit()

    # Second registration attempt with a different handle
    fee2 = context.any.txn.asset_transfer(xfer_asset=asset, asset_amount=UInt64(price), asset_receiver=app_addr, sender=sender)
    second_handle = context.any.string(length=6)
    reg2 = context.txn.defer_app_call(contract.register, second_handle)
    for txn in reg2._txns:  # type: ignore[attr-defined]
        txn.fields.setdefault("assets", []).append(asset)
    with context.txn.create_group(gtxns=[fee2, reg2]):
        reg2.submit()

    # Assert the handle remains the first one
    handle_key = b"Handle"
    stored_handle = context.ledger.get_local_state(contract.__app_id__, sender, handle_key)
    assert stored_handle.decode() == str(first_handle)
