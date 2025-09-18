from collections.abc import Iterator

import pytest
import algosdk
from algopy_testing import AlgopyTestContext, algopy_testing_context
from _algopy_testing.primitives.uint64 import UInt64

from smart_contracts.bingle_dapp.contract import BingleDapp


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
