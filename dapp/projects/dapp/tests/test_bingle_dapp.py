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


def test_migrate_reserve_transfers_to_new_app(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    new_app = ctx.any.application()
    contract.migrate_reserve(new_app)
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == Application(new_app.id).address
    assert itxn.amount == UInt64(1_000_000 - MIN_BALANCE)


def test_migrate_reserve_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.migrate_reserve(ctx.any.application())


def test_migrate_reserve_nothing_available_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, MIN_BALANCE)
    with pytest.raises(AssertionError):
        contract.migrate_reserve(ctx.any.application())


def test_set_predecessor_app_by_creator(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    assert contract.predecessor_app.value == old_app.id


def test_set_predecessor_app_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_predecessor_app(ctx.any.application())


def test_migrate_local_fails_without_predecessor(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    with pytest.raises(AssertionError):
        contract.migrate_local(ctx.any.application())


def test_migrate_local_fails_with_wrong_app(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    wrong_app = ctx.any.application()
    with pytest.raises(AssertionError):
        contract.migrate_local(wrong_app)


def test_migrate_local_copies_handle(ctx: AlgopyTestContext) -> None:
    from algopy import Application, String
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"Handle", b"alice")
    ctx.ledger.set_local_state(old_app, user, b"HandleTime", 1000)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.handle[user] == String("alice")
    assert contract.handle_time[user] == UInt64(1000)
    assert contract.last_handle_time.value == UInt64(1000)


def test_migrate_local_handle_time_bumped_when_conflict(ctx: AlgopyTestContext) -> None:
    from algopy import Application, String
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    # simulate new app already having a later registration
    contract.last_handle_time.value = UInt64(2000)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"Handle", b"bob")
    ctx.ledger.set_local_state(old_app, user, b"HandleTime", 1000)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.handle[user] == String("bob")
    assert contract.handle_time[user] == UInt64(2001)
    assert contract.last_handle_time.value == UInt64(2001)


def test_migrate_local_skips_handle_if_already_registered(ctx: AlgopyTestContext) -> None:
    from algopy import Application, String
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    # pre-register on new app
    contract.handle[user] = String("existing")
    contract.handle_time[user] = UInt64(500)
    ctx.ledger.set_local_state(old_app, user, b"Handle", b"old_handle")
    ctx.ledger.set_local_state(old_app, user, b"HandleTime", 200)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.handle[user] == String("existing")
    assert contract.handle_time[user] == UInt64(500)


def test_migrate_local_copies_allow_static_and_endpoint(ctx: AlgopyTestContext) -> None:
    from algopy import Application, String
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"allow_static", 1)
    ctx.ledger.set_local_state(old_app, user, b"static_endpoint", b"https://example.com/ep")
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.allow_static[user] == UInt64(1)
    assert contract.static_endpoint[user] == String("https://example.com/ep")


def test_migrate_local_does_not_copy_endpoint_when_allow_static_zero(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"allow_static", 0)
    ctx.ledger.set_local_state(old_app, user, b"static_endpoint", b"https://example.com/ep")
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.allow_static[user] == UInt64(0)
    _, exists = contract.static_endpoint.maybe(user)
    assert not exists


def test_migrate_local_copies_allow_relay(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"allow_relay", 1)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.allow_relay[user] == UInt64(1)
