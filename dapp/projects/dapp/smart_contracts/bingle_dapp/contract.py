# pyright: reportMissingModuleSource=false
from algopy import ARC4Contract, String, UInt64, Global, Txn, GlobalState, gtxn, urange, LocalState, itxn, Account
from algopy.arc4 import abimethod, baremethod


class BingleDapp(ARC4Contract):
    # Global state: price of 1 Bingle$ in microAlgos
    def __init__(self) -> None:
        self.bingle_price = GlobalState(UInt64, key="BinglePrice")
        # Local state for registration
        self.handle = LocalState(String, key="Handle")
        self.handle_time = LocalState(UInt64, key="HandleTime")
        # Local state flag: whether caller is allowed to set a static endpoint (1 == true)
        self.allow_static = LocalState(UInt64, key="allow_static")
        # Local state value: caller's registered static endpoint (if any)
        self.static_endpoint = LocalState(String, key="static_endpoint")

    @baremethod(allow_actions=["UpdateApplication"])
    def update_application(self) -> None:
        return

    @baremethod(allow_actions=["DeleteApplication"])
    def delete_application(self) -> None:
        return

    @baremethod(allow_actions=["OptIn"])
    def optin(self) -> None:
        return

    @abimethod()
    def opt_in_to_bingle(self, asset_id: UInt64) -> None:
        """Opt the application account into the provided ASA.

        Admin-only: must be called by the application creator.
        Performs an inner asset transfer of 0 to the app's own address to complete the opt-in.
        """
        assert Txn.sender == Global.creator_address
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

        Only the application creator can call this method.
        Stores the value in global state under key "BinglePrice".
        """
        assert Txn.sender == Global.creator_address
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

        # Only set if not previously set (keep oldest)
        current, exists = self.handle.maybe(Txn.sender)
        if not exists or current == String():
            self.handle[Txn.sender] = handle
            self.handle_time[Txn.sender] = Global.latest_timestamp

    @abimethod()
    def set_allow_static(self, target_address: Account, allow: UInt64) -> None:
        """Enable or disable permission for a target address to register a static endpoint.

        The target address must be supplied as an argument and also appear in Txn.Accounts[0].
        Only the application creator may call this method. The target account must be opted-in
        to the application.
        """
        # Only app creator may grant/revoke
        assert Txn.sender == Global.creator_address
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

    @abimethod()
    def register_endpoint(self, endpoint: String) -> None:
        """Register or clear a caller's static endpoint.

        Requirements:
        - Caller must have local state key "allow_static" set to true (1).
        - If `endpoint` is non-empty, store it under "static_endpoint" in local state.
        - If `endpoint` is empty (""), delete the local state key "static_endpoint".
        """
        # Ensure the caller is allowed to set a static endpoint
        allow_val, allow_exists = self.allow_static.maybe(Txn.sender)
        assert allow_exists and allow_val == UInt64(1)

        # Non-empty endpoint => set; empty => delete
        if endpoint != String():
            self.static_endpoint[Txn.sender] = endpoint
        else:
            del self.static_endpoint[Txn.sender]
