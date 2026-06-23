import algosdk.logic
import pytest
from algopy import OnCompleteAction, UInt64
from algopy_testing import AlgopyTestContext, algopy_testing_context

from smart_contracts.bingle_dapp.contract import BingleDapp

MIN_BALANCE = 100_000


@pytest.fixture()
def ctx() -> AlgopyTestContext:
    with algopy_testing_context() as context:
        yield context


def _deploy(ctx: AlgopyTestContext) -> tuple[BingleDapp, object, object]:
    """Return (contract, admin_account, withdrawer_account)."""
    contract = BingleDapp()
    admin = ctx.any.account()
    withdrawer = ctx.any.account()
    contract.create(admin, withdrawer)
    return contract, admin, withdrawer


def _fund_app(ctx: AlgopyTestContext, contract: BingleDapp, balance: int) -> None:
    app_address = algosdk.logic.get_application_address(contract.__app_id__)
    ctx.ledger.update_account(app_address, balance=UInt64(balance), min_balance=UInt64(MIN_BALANCE))


def test_update_application_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.UpdateApplication}
    ):
        contract.update_application()


def test_update_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(
        active_txn_overrides={
            "on_completion": OnCompleteAction.UpdateApplication,
            "sender": non_creator,
        }
    ):
        with pytest.raises(AssertionError):
            contract.update_application()


def test_delete_application_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.DeleteApplication}
    ):
        contract.delete_application()


def test_delete_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(
        active_txn_overrides={
            "on_completion": OnCompleteAction.DeleteApplication,
            "sender": non_creator,
        }
    ):
        with pytest.raises(AssertionError):
            contract.delete_application()


def test_create_sets_admin_and_withdrawer(ctx: AlgopyTestContext) -> None:
    contract = BingleDapp()
    admin = ctx.any.account()
    withdrawer = ctx.any.account()
    contract.create(admin, withdrawer)
    assert contract.app_admin.value == admin
    assert contract.app_withdrawer.value == withdrawer


def test_set_app_admin_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    new_admin = ctx.any.account()
    contract.set_app_admin(new_admin)
    assert contract.app_admin.value == new_admin


def test_set_app_admin_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_app_admin(ctx.any.account())


def test_set_app_withdrawer_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    new_withdrawer = ctx.any.account()
    contract.set_app_withdrawer(new_withdrawer)
    assert contract.app_withdrawer.value == new_withdrawer


def test_set_app_withdrawer_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_app_withdrawer(ctx.any.account())


def test_withdraw_exact_amount(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    recipient = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        contract.withdraw(recipient, UInt64(500_000))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == recipient
    assert itxn.amount == UInt64(500_000)


def test_withdraw_capped_to_balance_minus_min(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    recipient = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        contract.withdraw(recipient, UInt64(2_000_000))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == recipient
    assert itxn.amount == UInt64(1_000_000 - MIN_BALANCE)


def test_withdraw_by_non_withdrawer_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    non_withdrawer = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_withdrawer}):
        with pytest.raises(AssertionError):
            contract.withdraw(ctx.any.account(), UInt64(500_000))


def test_withdraw_nothing_available_fails(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, MIN_BALANCE)
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        with pytest.raises(AssertionError):
            contract.withdraw(ctx.any.account(), UInt64(1))
