import algosdk.logic
import pytest
from algopy import OnCompleteAction, String, UInt64, op
from algopy_testing import AlgopyTestContext, algopy_testing_context

from smart_contracts.bingle_dapp.contract import (
    BIT_MIGRATED,
    BIT_RELAY,
    BIT_STATIC,
    BIT_SW_CLIENT,
    BIT_SW_NODE,
    BingleDapp,
)

MIN_BALANCE = 100_000
# migrate_reserve keeps one min_txn_fee back per potential inner txn so the app stays at min balance.
MIN_TXN_FEE = 1_000


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
        contract.withdraw(recipient, UInt64(500_000), UInt64(0), UInt64(0))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == recipient
    assert itxn.amount == UInt64(500_000)


def test_withdraw_capped_to_balance_minus_min(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    recipient = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        contract.withdraw(recipient, UInt64(2_000_000), UInt64(0), UInt64(0))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == recipient
    assert itxn.amount == UInt64(1_000_000 - MIN_BALANCE)


def test_withdraw_by_non_withdrawer_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    non_withdrawer = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_withdrawer}):
        with pytest.raises(AssertionError):
            contract.withdraw(ctx.any.account(), UInt64(500_000), UInt64(0), UInt64(0))


def test_withdraw_nothing_available_fails(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, MIN_BALANCE)
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        with pytest.raises(AssertionError):
            contract.withdraw(ctx.any.account(), UInt64(1), UInt64(0), UInt64(0))


def test_withdraw_asset(ctx: AlgopyTestContext) -> None:
    contract, _, withdrawer = _deploy(ctx)
    app_address = algosdk.logic.get_application_address(contract.__app_id__)
    asset = ctx.any.asset()
    ctx.ledger.update_asset_holdings(asset, app_address, balance=500)
    recipient = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        contract.withdraw(recipient, UInt64(0), asset.id, UInt64(200))
    itxn = ctx.txn.last_group.last_itxn.asset_transfer
    assert itxn.asset_receiver == recipient
    assert itxn.asset_amount == UInt64(200)
    assert itxn.xfer_asset == asset


def test_migrate_reserve_transfers_to_new_app(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    new_app = ctx.any.application()
    contract.migrate_reserve(new_app, UInt64(0))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == Application(new_app.id).address
    # one min_txn_fee is held back for the payment inner txn
    assert itxn.amount == UInt64(1_000_000 - MIN_BALANCE - MIN_TXN_FEE)


def test_migrate_reserve_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.migrate_reserve(ctx.any.application(), UInt64(0))


def test_migrate_reserve_with_zero_algo_is_noop(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, MIN_BALANCE)
    contract.migrate_reserve(ctx.any.application(), UInt64(0))


def test_migrate_reserve_transfers_asset(ctx: AlgopyTestContext) -> None:
    from algopy import Application
    contract, _, _ = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    app_address = algosdk.logic.get_application_address(contract.__app_id__)
    asset = ctx.any.asset()
    ctx.ledger.update_asset_holdings(asset, app_address, balance=9_000)
    new_app = ctx.any.application()
    contract.migrate_reserve(new_app, asset.id)
    asset_itxn = ctx.txn.last_group.last_itxn.asset_transfer
    assert asset_itxn.asset_receiver == Application(new_app.id).address
    assert asset_itxn.asset_amount == UInt64(9_000)


def test_set_predecessor_app_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    # Lineage is the packed 8-byte id of the immediate predecessor.
    assert contract.ancestor_apps.value == op.itob(old_app.id)


def test_set_predecessor_app_accumulates_lineage(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    grandparent = ctx.any.application()
    parent = ctx.any.application()
    # parent (deployed with the new contract) already carries grandparent in its lineage
    ctx.ledger.set_global_state(parent, b"AncestorApps", op.itob(grandparent.id))
    contract.set_predecessor_app(parent)
    assert contract.ancestor_apps.value == op.itob(parent.id) + op.itob(grandparent.id)


def test_set_predecessor_app_bridges_legacy_predecessor(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    legacy_parent = ctx.any.application()
    legacy_grandparent = ctx.any.application()
    # legacy_parent was deployed with the OLD contract: a single PredecessorApp pointer
    ctx.ledger.set_global_state(legacy_parent, b"PredecessorApp", legacy_grandparent.id)
    contract.set_predecessor_app(legacy_parent)
    assert contract.ancestor_apps.value == op.itob(legacy_parent.id) + op.itob(legacy_grandparent.id)


def test_migrate_local_accepts_non_immediate_ancestor(ctx: AlgopyTestContext) -> None:
    from algopy import String
    contract, _, _ = _deploy(ctx)
    grandparent = ctx.any.application()
    parent = ctx.any.application()
    ctx.ledger.set_global_state(parent, b"AncestorApps", op.itob(grandparent.id))
    contract.set_predecessor_app(parent)  # lineage = {parent, grandparent}
    user = ctx.any.account()
    # user's data lives two versions back, on the grandparent
    ctx.ledger.set_local_state(grandparent, user, b"Handle", b"carol")
    ctx.ledger.set_local_state(grandparent, user, b"HandleTime", 1000)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(grandparent)
    assert contract.handle[user] == String("carol")
    assert contract.handle_time[user] == UInt64(1000)


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
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    # Old app holds the legacy scalar encoding; migrate folds it into the packed field + sentinel.
    ctx.ledger.set_local_state(old_app, user, b"allow_static", 1)
    ctx.ledger.set_local_state(old_app, user, b"static_endpoint", b"https://example.com/ep")
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_STATIC)
    assert contract.static_endpoint[user] == String("https://example.com/ep")


def test_migrate_local_does_not_copy_endpoint_when_allow_static_zero(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"allow_static", 0)
    ctx.ledger.set_local_state(old_app, user, b"static_endpoint", b"https://example.com/ep")
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    # Migrated (sentinel set) but the static bit is clear, so the endpoint is not copied.
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED)
    _, exists = contract.static_endpoint.maybe(user)
    assert not exists


def test_migrate_local_copies_allow_relay(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    ctx.ledger.set_local_state(old_app, user, b"allow_relay", 1)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    # Legacy relay grant folds into the packed field's relay bit (no separate allow_relay write).
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_RELAY)


def test_migrate_local_copies_packed_field_verbatim(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    old_app = ctx.any.application()
    contract.set_predecessor_app(old_app)
    user = ctx.any.account()
    # Old app already ran this contract version: its allow_static slot holds the packed bitfield
    # (with sw bits). migrate must carry those over verbatim, not re-fold them.
    packed = BIT_MIGRATED | BIT_STATIC | BIT_SW_NODE | BIT_SW_CLIENT
    ctx.ledger.set_local_state(old_app, user, b"allow_static", packed)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.migrate_local(old_app)
    assert contract.allow_static[user] == UInt64(packed)


def test_set_allow_static_sets_bit_and_migrates(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(1))
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_STATIC)


def test_set_allow_relay_sets_bit(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_relay(user, UInt64(1))
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_RELAY)


def test_set_allow_sw_node_sets_bit(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_sw_node(user, UInt64(1))
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_SW_NODE)


def test_set_allow_sw_client_sets_bit(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_sw_client(user, UInt64(1))
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_SW_CLIENT)


def test_set_allow_preserves_other_bits(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(1))
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_sw_node(user, UInt64(1))
    # Second setter leaves the static bit intact.
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_STATIC | BIT_SW_NODE)


def test_set_allow_clear_bit_keeps_others(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(1))
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_relay(user, UInt64(1))
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_relay(user, UInt64(0))
    # Clearing relay leaves static set and the sentinel intact.
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED | BIT_STATIC)


def test_set_allow_folds_legacy_scalars_on_first_write(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    # An un-migrated account holding the legacy separate scalars in this app.
    ctx.ledger.set_local_state(contract, user, b"allow_static", 1)
    ctx.ledger.set_local_state(contract, user, b"allow_relay", 1)
    # First write migrates: folds both legacy grants into the packed field and adds the new bit.
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_sw_client(user, UInt64(1))
    assert contract.allow_static[user] == UInt64(
        BIT_MIGRATED | BIT_STATIC | BIT_RELAY | BIT_SW_CLIENT
    )


def test_set_allow_by_non_admin_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    user = ctx.any.account()
    non_admin = ctx.any.account()
    with pytest.raises(Exception):
        with ctx.txn.create_group(active_txn_overrides={"sender": non_admin}):
            contract.set_allow_sw_node(user, UInt64(1))


def test_set_allow_static_clear_deletes_endpoint(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(1))
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.register_endpoint(String("1.2.3.4:5678"))
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(0))
    assert contract.allow_static[user] == UInt64(BIT_MIGRATED)
    _, exists = contract.static_endpoint.maybe(user)
    assert not exists


def test_register_endpoint_allowed_via_legacy_scalar(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    user = ctx.any.account()
    # Un-migrated legacy allow_static scalar (sentinel clear) must still authorise the caller.
    ctx.ledger.set_local_state(contract, user, b"allow_static", 1)
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.register_endpoint(String("1.2.3.4:5678"))
    assert contract.static_endpoint[user] == String("1.2.3.4:5678")


def test_register_endpoint_allowed_via_packed_bit(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_static(user, UInt64(1))
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        contract.register_endpoint(String("1.2.3.4:5678"))
    assert contract.static_endpoint[user] == String("1.2.3.4:5678")


def test_register_endpoint_denied_without_static_bit(ctx: AlgopyTestContext) -> None:
    contract, admin, _ = _deploy(ctx)
    user = ctx.any.account()
    # Granting only relay leaves the static bit clear, so register_endpoint must be rejected.
    with ctx.txn.create_group(active_txn_overrides={"sender": admin}):
        contract.set_allow_relay(user, UInt64(1))
    with pytest.raises(Exception):
        with ctx.txn.create_group(active_txn_overrides={"sender": user}):
            contract.register_endpoint(String("1.2.3.4:5678"))


def test_set_successor_app_by_creator(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    successor = ctx.any.application()
    contract.set_successor_app(successor)
    assert contract.successor_app.value == op.itob(successor.id)


def test_set_successor_app_by_non_creator_fails(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    non_creator = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": non_creator}):
        with pytest.raises(AssertionError):
            contract.set_successor_app(ctx.any.application())


def test_register_fails_when_superseded(ctx: AlgopyTestContext) -> None:
    from algopy import String
    contract, _, _ = _deploy(ctx)
    contract.set_successor_app(ctx.any.application())
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        with pytest.raises(AssertionError):
            contract.register(String("dave"))


def test_buy_bingle_fails_when_superseded(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    contract.set_successor_app(ctx.any.application())
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        with pytest.raises(AssertionError):
            contract.buy_bingle()


def test_sell_bingle_fails_when_superseded(ctx: AlgopyTestContext) -> None:
    contract, _, _ = _deploy(ctx)
    contract.set_successor_app(ctx.any.application())
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        with pytest.raises(AssertionError):
            contract.sell_bingle(UInt64(1))


def test_register_endpoint_fails_when_superseded(ctx: AlgopyTestContext) -> None:
    from algopy import String
    contract, _, _ = _deploy(ctx)
    contract.set_successor_app(ctx.any.application())
    user = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": user}):
        with pytest.raises(AssertionError):
            contract.register_endpoint(String("1.2.3.4:5678"))


def test_withdraw_still_works_when_superseded(ctx: AlgopyTestContext) -> None:
    # Admin/creator winddown paths stay callable after the app is superseded so the old
    # app can be drained; only the user-facing state-changing methods are hard-blocked.
    contract, _, withdrawer = _deploy(ctx)
    _fund_app(ctx, contract, 1_000_000)
    contract.set_successor_app(ctx.any.application())
    recipient = ctx.any.account()
    with ctx.txn.create_group(active_txn_overrides={"sender": withdrawer}):
        contract.withdraw(recipient, UInt64(500_000), UInt64(0), UInt64(0))
    itxn = ctx.txn.last_group.last_itxn.payment
    assert itxn.receiver == recipient
    assert itxn.amount == UInt64(500_000)
