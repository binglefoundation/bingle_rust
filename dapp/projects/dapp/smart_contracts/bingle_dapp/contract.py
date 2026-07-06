# pyright: reportMissingModuleSource=false
from algopy import ARC4Contract, Application, String, UInt64, Global, Txn, GlobalState, gtxn, urange, LocalState, itxn, Account, Bytes, op, subroutine
from algopy.arc4 import abimethod, baremethod


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
        # Local state flag: whether caller is allowed to set a static endpoint (1 == true)
        self.allow_static = LocalState(UInt64, key="allow_static")
        # Local state value: caller's registered static endpoint (if any)
        self.static_endpoint = LocalState(String, key="static_endpoint")
        self.static_endpoint_x = LocalState(String, key="static_endpoint_x")
        # Local state flag: whether caller is allowed to relay (1 == true)
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

        The target address must be supplied as an argument and also appear in Txn.Accounts[0].
        Only the application creator may call this method. The target account must be opted-in
        to the application.
        """
        assert Txn.sender == self.app_admin.value
        # Optional consistency check: the provided address must match Txn.accounts[0]
        # (Not when we pass the creator in accounts)
        # assert target_address == Txn.accounts(0)
        # Normalize to 0/1
        val = UInt64(1) if allow != UInt64(0) else UInt64(0)
        # Target from foreign accounts (first account)
        self.allow_static[target_address] = val
        # If disabling permission, also clear any existing static_endpoint for the target
        if val == UInt64(0):
            _cur, exists = self.static_endpoint.maybe(target_address)
            if exists:
                del self.static_endpoint[target_address]
            _cur_x, exists_x = self.static_endpoint_x.maybe(target_address)
            if exists_x:
                del self.static_endpoint_x[target_address]

    @abimethod()
    def set_allow_relay(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to relay.

        The target address must be supplied as an argument.
        Only the application creator may call this method. The target account must be opted-in
        to the application.
        """
        assert Txn.sender == self.app_admin.value
        # Normalize to 0/1
        val = UInt64(1) if allow != UInt64(0) else UInt64(0)
        # Target from foreign accounts
        self.allow_relay[target_address] = val

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

        allow_static, allow_relay: copied as-is (they were admin-granted on the old app).
        static_endpoint / static_endpoint_x: only copied when allow_static == 1.
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
        if has_allow_static:
            self.allow_static[sender] = old_allow_static
            if old_allow_static == UInt64(1):
                old_endpoint, has_endpoint = op.AppLocal.get_ex_bytes(sender, old_app, b"static_endpoint")
                if has_endpoint:
                    self.static_endpoint[sender] = String.from_bytes(old_endpoint)
                old_endpoint_x, has_endpoint_x = op.AppLocal.get_ex_bytes(sender, old_app, b"static_endpoint_x")
                if has_endpoint_x:
                    self.static_endpoint_x[sender] = String.from_bytes(old_endpoint_x)

        old_allow_relay, has_allow_relay = op.AppLocal.get_ex_uint64(sender, old_app, b"allow_relay")
        if has_allow_relay:
            self.allow_relay[sender] = old_allow_relay

    @abimethod()
    def register_endpoint(self, endpoint: String) -> None:
        """Register or clear a caller's static endpoint.

        Requirements:
        - Caller must have local state key "allow_static" set to true (1).
        - If `endpoint` is non-empty, store it under "static_endpoint" and
          "static_endpoint_x" (if needed) in local state.
        - If `endpoint` is empty (""), delete both local state keys.
        """
        # Reject once superseded: force the client to upgrade to the successor app.
        assert not self._is_superseded()
        # Ensure the caller is allowed to set a static endpoint
        allow_val, allow_exists = self.allow_static.maybe(Txn.sender)
        assert allow_exists and allow_val == UInt64(1)

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
            _cur, exists = self.static_endpoint.maybe(Txn.sender)
            if exists:
                del self.static_endpoint[Txn.sender]
            _cur_x, exists_x = self.static_endpoint_x.maybe(Txn.sender)
            if exists_x:
                del self.static_endpoint_x[Txn.sender]
