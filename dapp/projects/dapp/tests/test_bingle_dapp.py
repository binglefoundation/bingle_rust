import pytest
from algopy import OnCompleteAction
from algopy_testing import AlgopyTestContext, algopy_testing_context

from smart_contracts.bingle_dapp.contract import BingleDapp


@pytest.fixture()
def ctx() -> AlgopyTestContext:
    with algopy_testing_context() as context:
        yield context


def _deploy(ctx: AlgopyTestContext) -> BingleDapp:
    """Instantiate and create the contract with fresh admin/withdrawer accounts."""
    contract = BingleDapp()
    contract.create(ctx.any.account(), ctx.any.account())
    return contract


def test_update_application_by_creator(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.UpdateApplication}
    ):
        contract.update_application()


def test_update_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
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
    contract = _deploy(ctx)
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.DeleteApplication}
    ):
        contract.delete_application()


def test_delete_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
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
    contract = _deploy(ctx)
    new_admin = ctx.any.account()
    contract.set_app_admin(new_admin)
    assert contract.app_admin.value == new_admin


def test_set_app_admin_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_app_admin(ctx.any.account())


def test_set_app_withdrawer_by_creator(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
    new_withdrawer = ctx.any.account()
    contract.set_app_withdrawer(new_withdrawer)
    assert contract.app_withdrawer.value == new_withdrawer


def test_set_app_withdrawer_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_app_withdrawer(ctx.any.account())
