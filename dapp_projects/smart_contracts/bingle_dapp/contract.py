# pyright: reportMissingModuleSource=false
from algopy import ARC4Contract, Application, String, UInt64, Global, Txn, GlobalState, gtxn, urange, LocalState, itxn, Account, Bytes, op, subroutine
from algopy.arc4 import abimethod, baremethod


# --- Allow-flag bitfield -----------------------------------------------------------------------
# The per-account "allow" permissions are packed into a single UInt64 local-state value, stored
# under the existing `allow_static` key. Each flag is one bit. Bit 63 (the most significant bit) is
# a MIGRATED sentinel that distinguishes this packed encoding from the pre-bitfield legacy encoding
# (two separate allow_static / allow_relay uint slots holding 0/1): set => the value is the packed
# bitfield; clear => the account still holds the legacy scalar. Migration is lazy and per-account —
# the first set_allow_* write folds the legacy scalars into bits 0/1 and sets bit 63 (see
# `_set_allow_bit`); readers fold on the fly (see `_packed_allow_bits`) — so no bulk migration
# pass is needed and no previously granted flag is lost.
#
# These bit positions are the on-chain contract consumed off-chain by the bingle_core readers and
# the Sidewinder membership reader (issue #232); keep them in sync with
# `bingle_core::blockchain::algo_bingle` allow-flag constants.
BIT_STATIC = 1 << 0     # permitted to register a static endpoint
BIT_RELAY = 1 << 1      # permitted to relay
BIT_SW_NODE = 1 << 2    # permitted as a Sidewinder cluster node
BIT_SW_CLIENT = 1 << 3  # permitted as a Sidewinder API client
# bits 4..62 are reserved for future flags.
BIT_MIGRATED = 1 << 63  # sentinel: set => packed bitfield; clear => legacy allow_static/relay scalar


class BingleDapp(ARC4Contract):
    # Global state: price of 1 Bingle$ in microAlgos
    def __init__(self) -> None:
        self.bingle_price = GlobalState(UInt64, key="BinglePrice")
        self.last_handle_time = GlobalState(UInt64, key="LastHandleTime")
        self.app_admin = GlobalState(Account, key="AppAdmin")
        self.app_withdrawer = GlobalState(Account, key="AppWithdrawer")
        # Local state for registration
        self.handle = LocalState(String, key="Handle")
        self.handle_time = LocalState(UInt64, key="HandleTime")
        # Local state: the packed allow-flag bitfield (see the module-level BIT_* constants). Reuses
        # the historical `allow_static` uint slot; once an account is migrated (bit 63 set) this one
        # value holds all four allow flags, so num_uints does not grow for the two new flags.
        self.allow_static = LocalState(UInt64, key="allow_static")
        # Local state value: caller's registered static endpoint (if any)
        self.static_endpoint = LocalState(String, key="static_endpoint")
        self.static_endpoint_x = LocalState(String, key="static_endpoint_x")
        # Legacy local state: the pre-bitfield allow_relay scalar. Kept declared so the local uint
        # schema count is unchanged (an in-place UpdateApplication cannot alter the schema); no longer
        # written — it is only read to fold an un-migrated account's relay grant into the packed field.
        self.allow_relay = LocalState(UInt64, key="allow_relay")
        # Accepted-ancestor lineage: a packed list of 8-byte big-endian app ids of every
        # creator-blessed source app that migrate_local will copy from. Prevents privilege
        # forging by only honouring ancestors the creator has explicitly blessed via
        # set_predecessor_app. Bounded by the 128-byte global value limit (~15 ancestors),
        # which is ample for the interim migration solution.
        self.ancestor_apps = GlobalState(Bytes, key="AncestorApps")
        # Successor app pointer: an 8-byte big-endian app id (empty == not superseded). When
        # set (via set_successor_app on this app), the app is superseded by a newer deployment:
        # user-facing state-changing methods hard-reject, and clients read this to prompt the
        # user to upgrade. Same encoding as AncestorApps so app_global_bytes can read it.
        self.successor_app = GlobalState(Bytes, key="SuccessorApp")

        # --- Reserved spare schema slots -------------------------------------------------
        # An app's state schema is fixed at creation and cannot be grown by UpdateApplication.
        # We therefore claim spare global/local capacity now so a future contract version can
        # be deployed as an in-place update and populate real fields into these slots without a
        # schema break or a full app-replacement migration. A reserved slot may be repurposed
        # by a later version as long as its type (uint vs byte-slice) is preserved, so the
        # compiled schema counts stay identical.
        #
        # These proxies are intentionally never read or written: declaring them is enough for
        # puyapy to count them in the compiled schema, which makes this contract the single
        # source of truth for the reserved capacity. See the schema in BingleDapp.arc56.json.
        #
        # Reserved: 8 global slots (4 uint + 4 byte-slice), 4 local slots (2 uint + 2 byte-slice).
        self.reserved_global_int_0 = GlobalState(UInt64, key="rsvd_g_i0")
        self.reserved_global_int_1 = GlobalState(UInt64, key="rsvd_g_i1")
        self.reserved_global_int_2 = GlobalState(UInt64, key="rsvd_g_i2")
        self.reserved_global_int_3 = GlobalState(UInt64, key="rsvd_g_i3")
        self.reserved_global_bytes_0 = GlobalState(Bytes, key="rsvd_g_b0")
        self.reserved_global_bytes_1 = GlobalState(Bytes, key="rsvd_g_b1")
        self.reserved_global_bytes_2 = GlobalState(Bytes, key="rsvd_g_b2")
        self.reserved_global_bytes_3 = GlobalState(Bytes, key="rsvd_g_b3")
        self.reserved_local_int_0 = LocalState(UInt64, key="rsvd_l_i0")
        self.reserved_local_int_1 = LocalState(UInt64, key="rsvd_l_i1")
        self.reserved_local_bytes_0 = LocalState(Bytes, key="rsvd_l_b0")
        self.reserved_local_bytes_1 = LocalState(Bytes, key="rsvd_l_b1")

    @abimethod(create="require")
    def create(self, app_admin: Account, app_withdrawer: Account) -> None:
        self.app_admin.value = app_admin
        self.app_withdrawer.value = app_withdrawer

    @subroutine
    def _lineage_contains(self, lineage: Bytes, app_id: UInt64) -> bool:
        """True if app_id (as an 8-byte big-endian id) appears in the packed lineage."""
        target = op.itob(app_id)
        count = lineage.length // UInt64(8)
        found = False
        for i in urange(count):
            if op.extract(lineage, i * UInt64(8), UInt64(8)) == target:
                found = True
        return found

    @subroutine
    def _is_superseded(self) -> bool:
        """True if this app has been marked superseded via set_successor_app."""
        return self.successor_app.get(default=Bytes()).length != UInt64(0)

    @subroutine
    def _fold_allow(
        self, static_raw: UInt64, has_static: bool, relay_raw: UInt64, has_relay: bool
    ) -> UInt64:
        """Fold an (allow_static, allow_relay) pair into the packed bitfield.

        If `static_raw` already carries the MIGRATED sentinel it is the packed value and returned
        as-is (its relay / sw bits are already present). Otherwise the two legacy scalars are folded
        into bits 0/1 and the sentinel is set (allow_sw_node / allow_sw_client did not exist
        pre-migration, so they read 0). Shared by the on-account read/write path and migrate_local, so
        the fold arithmetic exists once.
        """
        if has_static and (static_raw & UInt64(BIT_MIGRATED)) != UInt64(0):
            return static_raw
        packed = UInt64(BIT_MIGRATED)
        if has_static and static_raw != UInt64(0):
            packed = packed | UInt64(BIT_STATIC)
        if has_relay and relay_raw != UInt64(0):
            packed = packed | UInt64(BIT_RELAY)
        return packed

    @subroutine
    def _packed_allow_bits(self, account: Account) -> UInt64:
        """An account's allow bitfield in packed form, folding the legacy encoding if the account is
        un-migrated (sentinel-aware). Test permission bits against the BIT_* masks; the MIGRATED bit
        may be set. Returns just the sentinel (no permission bits) for an account with no allow state.
        """
        static_raw, has_static = self.allow_static.maybe(account)
        relay_raw, has_relay = self.allow_relay.maybe(account)
        return self._fold_allow(static_raw, has_static, relay_raw, has_relay)

    @subroutine
    def _set_allow_bit(self, target: Account, mask: UInt64, allow: UInt64) -> None:
        """Admin-only: set (`allow != 0`) or clear one allow bit for `target`, migrating the account's
        encoding on first write (via `_packed_allow_bits`, so no previously granted flag is lost), then
        write the packed value. The admin check and the 0/1 normalisation live here so every
        set_allow_* method shares them.
        """
        assert Txn.sender == self.app_admin.value
        packed = self._packed_allow_bits(target)
        if allow != UInt64(0):
            packed = packed | mask
        else:
            packed = packed & ~mask
        self.allow_static[target] = packed

    @subroutine
    def _clear_endpoint(self, account: Account) -> None:
        """Delete both static-endpoint local-state keys for `account`, if present."""
        _cur, exists = self.static_endpoint.maybe(account)
        if exists:
            del self.static_endpoint[account]
        _cur_x, exists_x = self.static_endpoint_x.maybe(account)
        if exists_x:
            del self.static_endpoint_x[account]

    @baremethod(allow_actions=["UpdateApplication"])
    def update_application(self) -> None:
        assert Txn.sender == Global.creator_address

    @baremethod(allow_actions=["DeleteApplication"])
    def delete_application(self) -> None:
        assert Txn.sender == Global.creator_address

    @baremethod(allow_actions=["OptIn"])
    def optin(self) -> None:
        return

    @abimethod()
    def opt_in_to_bingle(self, asset_id: UInt64) -> None:
        """Opt the application account into the provided ASA.

        Admin-only: must be called by the application admin.
        Performs an inner asset transfer of 0 to the app's own address to complete the opt-in.
        """
        assert Txn.sender == self.app_admin.value
        # Inner transaction: axfer 0 of `asset_id` to current_application_address
        # Use Algopy itxn builder for AssetTransfer with named arguments and submit.
        itxn.AssetTransfer(
            xfer_asset=asset_id,
            asset_receiver=Global.current_application_address,
            asset_amount=UInt64(0),
            fee=Global.min_txn_fee,
        ).submit()

    @abimethod()
    def set_bingle_price(self, price: UInt64) -> None:
        """Set the Bingle$ price in microAlgos.

        Only the application admin can call this method.
        Stores the value in global state under key "BinglePrice".
        """
        assert Txn.sender == self.app_admin.value
        self.bingle_price.value = price

    @abimethod()
    def buy_bingle(self) -> None:
        """Buy 1 Bingle$.

        Requirements enforced via transaction group:
        - The app call must include at least one foreign asset; the first one is treated
          as the Bingle$ ASA to credit.
        - There must be a payment in the group to the application address for exactly the
          current Bingle$ price held in global state.

        After verifying the payment, the contract performs an inner transaction that
        clawbacks 1 unit of the Bingle$ ASA from the creator-held reserve to the caller.
        This requires the ASA's clawback address to be the application address.
        """
        # Reject once superseded: force the client to upgrade to the successor app.
        assert not self._is_superseded()
        # Ensure a foreign asset is supplied to identify Bingle$ ASA
        asset_id = Txn.assets(0)

        price = self.bingle_price.value
        app_addr = Global.current_application_address
        buyer = Txn.sender

        saw_payment = False

        # Scan the current transaction group for required payment
        for i in urange(Global.group_size):
            t = gtxn.Transaction(i)
            # Payment check: receiver is app and amount equals current price
            if t.receiver == app_addr and t.amount == price:
                saw_payment = True

        # Require payment
        assert saw_payment

        # Inner clawback of 1 unit from the creator reserve to the buyer
        itxn.AssetTransfer(
            xfer_asset=asset_id,
            asset_sender=Global.current_application_address,
            asset_receiver=buyer,
            asset_amount=UInt64(1),
            fee=Global.min_txn_fee,
        ).submit()

    @abimethod()
    def sell_bingle(self, amount: UInt64) -> None:
        """Sell Bingle$.

        Requirements enforced via transaction group:
        - The app call must include at least one foreign asset; the first one is treated
          as the Bingle$ ASA being sold.
        - There must be an asset transfer in the group that transfers exactly `amount`
          units of that asset from the caller (Txn.sender) to the application address.
        - There must be a payment in the group to the caller for exactly
          (current Bingle$ price * amount).

        Note: As with buy_bingle, the contract validates accompanying transactions rather
        than performing inner transfers. Any account may fund the payout as long as the
        amount is correct.
        """
        # Reject once superseded: force the client to upgrade to the successor app.
        assert not self._is_superseded()
        # Identify the ASA and compute payout
        asset_id = Txn.assets(0)
        price = self.bingle_price.value
        seller = Txn.sender
        app_addr = Global.current_application_address
        payout = price * amount

        saw_payment = False
        saw_axfer = False

        for i in urange(Global.group_size):
            t = gtxn.Transaction(i)
            # Payout payment to the seller for the correct amount
            if t.receiver == seller and t.amount == payout:
                saw_payment = True

            # Asset transfer of `amount` from seller to the app address
            if (
                t.xfer_asset == asset_id
                and t.sender == seller
                and t.asset_receiver == app_addr
                and t.asset_amount == amount
            ):
                saw_axfer = True

        assert saw_payment
        assert saw_axfer

    @abimethod()
    def withdraw(self, address: Account, amount: UInt64, asset_id: UInt64, asset_amount: UInt64) -> None:
        assert Txn.sender == self.app_withdrawer.value
        if amount > UInt64(0):
            app_addr = Global.current_application_address
            app_balance = app_addr.balance
            app_min = app_addr.min_balance
            withdrawable = app_balance - app_min if app_balance > app_min else UInt64(0)
            actual = amount if amount <= withdrawable else withdrawable
            assert actual > UInt64(0)
            itxn.Payment(receiver=address, amount=actual, fee=Global.min_txn_fee).submit()
        if asset_amount > UInt64(0):
            itxn.AssetTransfer(
                xfer_asset=asset_id,
                asset_receiver=address,
                asset_amount=asset_amount,
                fee=Global.min_txn_fee,
            ).submit()

    @abimethod()
    def register(self, handle: String) -> None:
        """Register a handle for the caller.

        Requirements:
        - Caller must be opted-in to the ASA (enforced off-chain; this method validates
          the presence of an ASA transfer proving the holding exists).
        - Caller must be opted-in to the app to write local state (Algorand enforces this).
        - A one-time payment of one Bingle$  is required; enforced
          by validating an asset transfer of exactly 1 of the referenced ASA
          from the caller to the application address in the same group.
        - Stores the handle in local storage under key "Handle" and the timestamp under
          key "HandleTime" set to Global.latest_timestamp(). If a handle is already set,
          it will not be overwritten (oldest handle is kept).
        """
        # Reject once superseded: force the client to upgrade to the successor app.
        assert not self._is_superseded()
        asset_id = Txn.assets(0)
        app_addr = Global.current_application_address
        sender = Txn.sender

        saw_fee = False
        for i in urange(Global.group_size):
            t = gtxn.Transaction(i)
            if (
                t.xfer_asset == asset_id
                and t.sender == sender
                and t.asset_receiver == app_addr
                and t.asset_amount == 1
            ):
                saw_fee = True
        assert saw_fee

        # Ensure HandleTime is unique and strictly increasing
        last_time = self.last_handle_time.get(default=UInt64(0))
        handle_time = Global.latest_timestamp
        if handle_time <= last_time:
            handle_time = last_time + 1
        self.last_handle_time.value = handle_time

        # Only set if not previously set (keep oldest)
        current, exists = self.handle.maybe(Txn.sender)
        if not exists or current == String():
            self.handle[Txn.sender] = handle
            self.handle_time[Txn.sender] = handle_time

    @abimethod()
    def set_allow_static(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to register a static endpoint.

        The target address must be supplied as an argument and also appear in the transaction's
        foreign accounts array. Only the application admin may call this method. The target account
        must be opted-in to the application. Sets/clears the allow_static bit in the packed allow
        field (migrating the account's encoding on first write). Admin-only (enforced in
        `_set_allow_bit`).
        """
        self._set_allow_bit(target_address, UInt64(BIT_STATIC), allow)
        # The static bit now equals `allow`; if it was cleared, also clear any existing static_endpoint.
        if allow == UInt64(0):
            self._clear_endpoint(target_address)

    @abimethod()
    def set_allow_relay(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to relay.

        The target address must be supplied as an argument and appear in the transaction's foreign
        accounts array. Only the application admin may call this method. The target account must be
        opted-in to the application. Sets/clears the allow_relay bit in the packed allow field.
        Admin-only (enforced in `_set_allow_bit`).
        """
        self._set_allow_bit(target_address, UInt64(BIT_RELAY), allow)

    @abimethod()
    def set_allow_sw_node(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to act as a Sidewinder cluster node.

        The target address must be supplied as an argument and appear in the transaction's foreign
        accounts array. Only the application admin may call this method. The target account must be
        opted-in to the application. Sets/clears the allow_sw_node bit in the packed allow field.
        Admin-only (enforced in `_set_allow_bit`).
        """
        self._set_allow_bit(target_address, UInt64(BIT_SW_NODE), allow)

    @abimethod()
    def set_allow_sw_client(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to act as a Sidewinder API client.

        The target address must be supplied as an argument and appear in the transaction's foreign
        accounts array. Only the application admin may call this method. The target account must be
        opted-in to the application. Sets/clears the allow_sw_client bit in the packed allow field.
        Admin-only (enforced in `_set_allow_bit`).
        """
        self._set_allow_bit(target_address, UInt64(BIT_SW_CLIENT), allow)

    @abimethod()
    def set_predecessor_app(self, predecessor: Application) -> None:
        """Bless `predecessor` (and its own ancestors) as migration sources for this app.

        Creator-only. Records the accepted-ancestor lineage as a packed list of 8-byte app
        ids: the immediate predecessor, plus the predecessor's own accumulated lineage
        (AncestorApps), plus — to bridge apps deployed with the older single-predecessor
        contract — the predecessor's legacy PredecessorApp pointer. migrate_local accepts any
        app in this lineage, so a user several versions behind migrates directly in one hop.
        `predecessor` must be included in the transaction's foreign apps array.
        """
        assert Txn.sender == Global.creator_address

        # Start the lineage with the immediate predecessor.
        lineage = op.itob(predecessor.id)

        # Carry forward the predecessor's own accumulated lineage (newer-contract apps),
        # de-duplicated.
        pred_anc, has_anc = op.AppGlobal.get_ex_bytes(predecessor, b"AncestorApps")
        if has_anc:
            anc_count = pred_anc.length // UInt64(8)
            for i in urange(anc_count):
                chunk = op.extract(pred_anc, i * UInt64(8), UInt64(8))
                if not self._lineage_contains(lineage, op.btoi(chunk)):
                    lineage += chunk

        # Bridge from an app deployed with the older contract that recorded only a single
        # PredecessorApp (uint64) rather than the AncestorApps lineage.
        old_pred, has_old = op.AppGlobal.get_ex_uint64(predecessor, b"PredecessorApp")
        if has_old and old_pred != UInt64(0) and not self._lineage_contains(lineage, old_pred):
            lineage += op.itob(old_pred)

        self.ancestor_apps.value = lineage

    @abimethod()
    def set_successor_app(self, successor: Application) -> None:
        """Mark this app as superseded by `successor`, forcing clients to upgrade.

        Creator-only. Records `successor`'s id as the SuccessorApp global (8-byte big-endian).
        Once set, the user-facing state-changing methods (register, buy_bingle, sell_bingle,
        register_endpoint) hard-reject, and clients read this pointer on start to prompt the
        user to update. Admin/creator methods, withdraw, and the migrate_* methods stay
        callable so the old app can still be wound down and users migrated. Re-pointable.
        `successor` must be included in the transaction's foreign apps array.
        """
        assert Txn.sender == Global.creator_address
        self.successor_app.value = op.itob(successor.id)

    @abimethod()
    def set_app_admin(self, admin: Account) -> None:
        assert Txn.sender == Global.creator_address
        self.app_admin.value = admin

    @abimethod()
    def set_app_withdrawer(self, withdrawer: Account) -> None:
        assert Txn.sender == Global.creator_address
        self.app_withdrawer.value = withdrawer

    @abimethod()
    def migrate_global(self, old_app: Application) -> None:
        """Copy global state from old_app into this contract.

        Creator-only. Call once after deploying a new version to carry over
        BinglePrice, LastHandleTime, AppAdmin, and AppWithdrawer from the old app.
        old_app must be included in the transaction's foreign apps array.
        """
        assert Txn.sender == Global.creator_address

        price, exists = op.AppGlobal.get_ex_uint64(old_app, b"BinglePrice")
        if exists:
            self.bingle_price.value = price

        last_time, exists = op.AppGlobal.get_ex_uint64(old_app, b"LastHandleTime")
        if exists:
            self.last_handle_time.value = last_time

        admin_bytes, exists = op.AppGlobal.get_ex_bytes(old_app, b"AppAdmin")
        if exists:
            self.app_admin.value = Account(admin_bytes)

        withdrawer_bytes, exists = op.AppGlobal.get_ex_bytes(old_app, b"AppWithdrawer")
        if exists:
            self.app_withdrawer.value = Account(withdrawer_bytes)

    @abimethod()
    def migrate_reserve(self, new_app: Application, asset_id: UInt64) -> None:
        assert Txn.sender == Global.creator_address
        app_addr = Global.current_application_address
        app_balance = app_addr.balance
        app_min = app_addr.min_balance
        # Each inner txn with fee=Global.min_txn_fee deducts from the app account.
        # Reserve one fee slot per potential inner txn so the app stays at min_balance.
        fee_reserve = Global.min_txn_fee + (
            Global.min_txn_fee if asset_id != UInt64(0) else UInt64(0)
        )
        withdrawable = (
            app_balance - app_min - fee_reserve
            if app_balance > app_min + fee_reserve
            else UInt64(0)
        )
        if withdrawable > UInt64(0):
            itxn.Payment(
                receiver=new_app.address,
                amount=withdrawable,
                fee=Global.min_txn_fee,
            ).submit()
        if asset_id != UInt64(0):
            asa_balance, has_balance = op.AssetHoldingGet.asset_balance(app_addr, asset_id)
            if has_balance and asa_balance > UInt64(0):
                itxn.AssetTransfer(
                    xfer_asset=asset_id,
                    asset_receiver=new_app.address,
                    asset_amount=asa_balance,
                    fee=Global.min_txn_fee,
                ).submit()

    @abimethod()
    def migrate_local(self, old_app: Application) -> None:
        """Copy the caller's local state from old_app into this contract.

        old_app must be one of the creator-blessed ancestor apps recorded in the AncestorApps
        lineage (via set_predecessor_app), preventing a user from supplying a fake app they
        control to forge admin-granted permissions such as allow_static or allow_relay.

        handle/handle_time: first-write-wins (not overwritten if already registered here).
        The handle_time is preserved from the old app but bumped past last_handle_time
        if needed to avoid duplicate timestamps.

        allow flags: the admin-granted allow_static / allow_relay from the old app are folded into
        this contract's packed allow bitfield (with the MIGRATED sentinel set). If the old app already
        stored the packed encoding (it ran this contract version), it is copied verbatim so its
        allow_sw_node / allow_sw_client bits carry over too.
        static_endpoint / static_endpoint_x: only copied when the resulting allow_static bit is set.
        """
        lineage = self.ancestor_apps.get(default=Bytes())
        assert self._lineage_contains(lineage, old_app.id)

        sender = Txn.sender

        old_handle, has_handle = op.AppLocal.get_ex_bytes(sender, old_app, b"Handle")
        if has_handle:
            current, exists = self.handle.maybe(sender)
            if not exists or current == String():
                self.handle[sender] = String.from_bytes(old_handle)
                old_time, has_time = op.AppLocal.get_ex_uint64(sender, old_app, b"HandleTime")
                if has_time:
                    last_time = self.last_handle_time.get(default=UInt64(0))
                    handle_time = old_time if old_time > last_time else last_time + UInt64(1)
                    self.handle_time[sender] = handle_time
                    self.last_handle_time.value = handle_time

        old_allow_static, has_allow_static = op.AppLocal.get_ex_uint64(sender, old_app, b"allow_static")
        old_allow_relay, has_allow_relay = op.AppLocal.get_ex_uint64(sender, old_app, b"allow_relay")
        if has_allow_static or has_allow_relay:
            # Fold the ancestor's allow state into this contract's packed field (verbatim if the
            # ancestor already stored the packed encoding, else legacy scalars folded + sentinel).
            packed = self._fold_allow(
                old_allow_static, has_allow_static, old_allow_relay, has_allow_relay
            )
            self.allow_static[sender] = packed
            if (packed & UInt64(BIT_STATIC)) != UInt64(0):
                old_endpoint, has_endpoint = op.AppLocal.get_ex_bytes(sender, old_app, b"static_endpoint")
                if has_endpoint:
                    self.static_endpoint[sender] = String.from_bytes(old_endpoint)
                old_endpoint_x, has_endpoint_x = op.AppLocal.get_ex_bytes(sender, old_app, b"static_endpoint_x")
                if has_endpoint_x:
                    self.static_endpoint_x[sender] = String.from_bytes(old_endpoint_x)

    @abimethod()
    def register_endpoint(self, endpoint: String) -> None:
        """Register or clear a caller's static endpoint.

        Requirements:
        - Caller must have the allow_static bit set (packed field, or the legacy allow_static scalar
          for an un-migrated account — decoded uniformly via `_effective_allow_bits`).
        - If `endpoint` is non-empty, store it under "static_endpoint" and
          "static_endpoint_x" (if needed) in local state.
        - If `endpoint` is empty (""), delete both local state keys.
        """
        # Reject once superseded: force the client to upgrade to the successor app.
        assert not self._is_superseded()
        # Ensure the caller is allowed to set a static endpoint (sentinel-aware decode)
        assert (self._packed_allow_bits(Txn.sender) & UInt64(BIT_STATIC)) != UInt64(0)

        # Non-empty endpoint => set; empty => delete
        if endpoint != String():
            bytes_val = endpoint.bytes
            if bytes_val.length > 64:
                self.static_endpoint[Txn.sender] = String.from_bytes(bytes_val[0:64])
                self.static_endpoint_x[Txn.sender] = String.from_bytes(bytes_val[64:])
            else:
                self.static_endpoint[Txn.sender] = endpoint
                _cur_x, exists_x = self.static_endpoint_x.maybe(Txn.sender)
                if exists_x:
                    del self.static_endpoint_x[Txn.sender]
        else:
            self._clear_endpoint(Txn.sender)
