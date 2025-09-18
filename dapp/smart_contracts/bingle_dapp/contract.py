# pyright: reportMissingModuleSource=false
from algopy import ARC4Contract, String, UInt64, Global, Txn, GlobalState
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
