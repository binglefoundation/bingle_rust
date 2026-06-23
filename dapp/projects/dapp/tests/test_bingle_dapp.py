import pytest
from algopy import OnCompleteAction
from algopy_testing import AlgopyTestContext, algopy_testing_context

from smart_contracts.bingle_dapp.contract import BingleDapp


@pytest.fixture()
def ctx() -> AlgopyTestContext:
    with algopy_testing_context() as context:
        yield context


def test_update_application_by_creator(ctx: AlgopyTestContext) -> None:
    contract = BingleDapp()
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.UpdateApplication}
    ):
        contract.update_application()


def test_update_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = BingleDapp()
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
    contract = BingleDapp()
    with ctx.txn.create_group(
        active_txn_overrides={"on_completion": OnCompleteAction.DeleteApplication}
    ):
        contract.delete_application()


def test_delete_application_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract = BingleDapp()
    non_creator = ctx.any.account()
    with ctx.txn.create_group(
        active_txn_overrides={
            "on_completion": OnCompleteAction.DeleteApplication,
            "sender": non_creator,
        }
    ):
        with pytest.raises(AssertionError):
            contract.delete_application()
