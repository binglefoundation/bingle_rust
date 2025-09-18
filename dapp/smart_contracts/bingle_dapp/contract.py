# pyright: reportMissingModuleSource=false
from algopy import ARC4Contract, String, UInt64, Global, Txn, GlobalState, gtxn, urange
from algopy.arc4 import abimethod


class BingleDapp(ARC4Contract):
    # Global state: price of 1 Bingle$ in microAlgos
    def __init__(self) -> None:
        self.bingle_price = GlobalState(UInt64, key="BinglePrice")

    @abimethod()
    def hello(self, name: String) -> String:
        return "Hello, " + name

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
        - There must be an asset transfer in the group that transfers exactly 1 unit of
          the referenced asset to the caller (Txn.sender).

        Note: The contract does not perform an inner transfer; instead it validates an
        accompanying asset transfer and payment in the same group. This allows flexible
        funding sources (creator, reserve, distributor) without specific ASA roles.
        """
        # Ensure a foreign asset is supplied to identify Bingle$ ASA
        asset_id = Txn.assets(0)

        price = self.bingle_price.value
        app_addr = Global.current_application_address
        buyer = Txn.sender

        saw_payment = False
        saw_axfer = False

        # Scan the current transaction group for required payment and asset transfer
        for i in urange(Global.group_size):
            t = gtxn.Transaction(i)
            # Payment check: receiver is app and amount equals current price
            if t.receiver == app_addr and t.amount == price:
                saw_payment = True

            # Asset transfer check: correct asset, receiver is buyer, amount == 1
            if (
                t.xfer_asset == asset_id
                and t.asset_receiver == buyer
                and t.asset_amount == UInt64(1)
            ):
                saw_axfer = True

        # Require both conditions
        assert saw_payment
        assert saw_axfer
