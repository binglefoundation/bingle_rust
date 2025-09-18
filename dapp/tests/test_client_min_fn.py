# This test exercises dapp/src/client.py by calling the Mini.fn ABI method via the generated client.
# It does not require a live Algorand node; instead, it stubs the minimal parts of algosdk/algokit_utils
# needed for import and for building method call params. The call result is produced by a fake AppClient.

from __future__ import annotations

import sys
import types
from dataclasses import dataclass
from pathlib import Path


def _install_stub_modules() -> None:
    """Install minimal stubs for algosdk and algokit_utils so that dapp/src/client.py can be imported
    without requiring the full Algorand toolchain during unit testing.
    """
    # ----- algosdk stubs -----
    algosdk = types.ModuleType("algosdk")

    transaction = types.ModuleType("algosdk.transaction")

    class OnComplete:
        NoOpOC = 0

    class Transaction:  # placeholder
        pass

    transaction.OnComplete = OnComplete
    transaction.Transaction = Transaction

    atomic_tc = types.ModuleType("algosdk.atomic_transaction_composer")

    class TransactionSigner:  # placeholder
        pass

    atomic_tc.TransactionSigner = TransactionSigner

    source_map = types.ModuleType("algosdk.source_map")

    class SourceMap:  # placeholder
        pass

    source_map.SourceMap = SourceMap

    v2client = types.ModuleType("algosdk.v2client")
    models = types.ModuleType("algosdk.v2client.models")

    class SimulateTraceConfig:  # placeholder
        pass

    models.SimulateTraceConfig = SimulateTraceConfig

    # Register algosdk package and submodules
    sys.modules.setdefault("algosdk", algosdk)
    sys.modules.setdefault("algosdk.transaction", transaction)
    sys.modules.setdefault("algosdk.atomic_transaction_composer", atomic_tc)
    sys.modules.setdefault("algosdk.source_map", source_map)
    sys.modules.setdefault("algosdk.v2client", v2client)
    sys.modules.setdefault("algosdk.v2client.models", models)

    # ----- algokit_utils stubs -----
    algokit_utils = types.ModuleType("algokit_utils")

    class Arc56Contract:
        @classmethod
        def from_json(cls, _json: str) -> "Arc56Contract":
            return cls()

        # Only needed for decode_return_value; our test doesn't use it
        def get_arc56_method(self, _method: str):  # pragma: no cover - not used in this test
            return None

    @dataclass
    class CommonAppCallParams:
        # Keep empty so dataclasses.asdict works
        pass

    @dataclass
    class AppClientMethodCallParams:
        method: str
        args: list | None = None

    # Type placeholders (not used at runtime in this test)
    class AppClient:  # pragma: no cover - placeholder for typing
        pass

    class SendParams:  # pragma: no cover - placeholder for typing
        pass

    class AppCallMethodCallParams:  # pragma: no cover - placeholder for typing
        pass

    class ABIReturn:  # pragma: no cover - placeholder for typing
        pass

    class ABIValue:  # pragma: no cover - placeholder for typing
        pass

    class ABIStruct:  # pragma: no cover - placeholder for typing
        pass

    class BuiltTransactions:  # pragma: no cover - placeholder for typing
        pass

    class AppClientBareCallParams:  # pragma: no cover - placeholder for typing
        def __init__(self, **kwargs):
            self.kwargs = kwargs

    class AppClientBareCallCreateParams(AppClientBareCallParams):  # placeholder
        pass

    class AppFactory:  # pragma: no cover - placeholder for typing
        pass

    class TypedAppFactoryProtocol:  # pragma: no cover - placeholder for typing
        pass

    class AppFactoryParams:  # pragma: no cover - placeholder for typing
        def __init__(self, **kwargs):
            self.kwargs = kwargs

    class AlgorandClient:  # pragma: no cover - placeholder for typing
        pass

    class AppFactoryCreateParams:  # pragma: no cover - placeholder for typing
        def __init__(self, **kwargs):
            self.kwargs = kwargs

    class AppClientCompilationParams:  # pragma: no cover - placeholder for typing
        pass

    class AppCallParams:  # pragma: no cover - placeholder for typing
        pass

    class SendAppTransactionResult(int):  # treat as int for simplicity
        pass

    class SendAtomicTransactionComposerResults:  # pragma: no cover - placeholder
        pass

    class TransactionComposer:  # pragma: no cover - placeholder for typing
        def simulate(self, **kwargs):  # type: ignore[no-untyped-def]
            return SendAtomicTransactionComposerResults()

    class ApplicationLookup:  # pragma: no cover - placeholder for typing
        pass

    class OnUpdate:  # pragma: no cover - placeholder for typing
        pass

    class SendAppCreateTransactionResult:  # pragma: no cover - placeholder for typing
        pass

    algokit_utils.Arc56Contract = Arc56Contract
    algokit_utils.CommonAppCallParams = CommonAppCallParams
    algokit_utils.AppClientMethodCallParams = AppClientMethodCallParams
    algokit_utils.AppClient = AppClient
    algokit_utils.SendParams = SendParams
    algokit_utils.AppCallMethodCallParams = AppCallMethodCallParams
    algokit_utils.ABIReturn = ABIReturn
    algokit_utils.ABIValue = ABIValue
    algokit_utils.ABIStruct = ABIStruct
    algokit_utils.BuiltTransactions = BuiltTransactions
    algokit_utils.AppClientBareCallParams = AppClientBareCallParams
    algokit_utils.AppClientBareCallCreateParams = AppClientBareCallCreateParams
    algokit_utils.AppFactory = AppFactory
    algokit_utils.TypedAppFactoryProtocol = TypedAppFactoryProtocol
    algokit_utils.AppFactoryParams = AppFactoryParams
    algokit_utils.AlgorandClient = AlgorandClient
    algokit_utils.AppFactoryCreateParams = AppFactoryCreateParams
    algokit_utils.AppClientCompilationParams = AppClientCompilationParams
    algokit_utils.AppCallParams = AppCallParams
    algokit_utils.SendAppTransactionResult = SendAppTransactionResult
    algokit_utils.SendAtomicTransactionComposerResults = SendAtomicTransactionComposerResults
    algokit_utils.TransactionComposer = TransactionComposer
    algokit_utils.ApplicationLookup = ApplicationLookup
    algokit_utils.OnUpdate = OnUpdate
    algokit_utils.SendAppCreateTransactionResult = SendAppCreateTransactionResult

    sys.modules.setdefault("algokit_utils", algokit_utils)


def test_mini_fn_via_client_with_tuple_args():
    # Arrange: make dapp/src importable and install stubs before importing client
    _install_stub_modules()

    repo_root = Path(__file__).resolve().parents[2]
    dapp_src = repo_root / "dapp" / "src"
    sys.path.insert(0, str(dapp_src))

    import client as client_mod  # type: ignore

    # Fake AppClient that returns 2*x+1 for Mini.fn
    class FakeSend:
        def call(self, params, send_params=None):  # pylint: disable=unused-argument
            # params is an AppClientMethodCallParams with .args = [x]
            args = getattr(params, "args", None)
            assert args is not None and len(args) == 1, "expected single arg to fn"
            x = args[0]
            return 2 * x + 1

    class FakeAppClient:
        def __init__(self):
            self.send = FakeSend()

    mini = client_mod.MiniClient(app_client=FakeAppClient())

    # Act
    result = mini.send.fn((5,))

    # Assert
    assert result == 11


def test_mini_fn_via_client_with_dataclass_args():
    # Arrange
    _install_stub_modules()

    repo_root = Path(__file__).resolve().parents[2]
    dapp_src = repo_root / "dapp" / "src"
    if str(dapp_src) not in sys.path:
        sys.path.insert(0, str(dapp_src))

    import client as client_mod  # type: ignore

    class FakeSend:
        def call(self, params, send_params=None):  # pylint: disable=unused-argument
            args = getattr(params, "args", None)
            assert args is not None and len(args) == 1
            x = args[0]
            return 2 * x + 1

    class FakeAppClient:
        def __init__(self):
            self.send = FakeSend()

    mini = client_mod.MiniClient(app_client=FakeAppClient())

    # Act: use the generated FnArgs dataclass
    result = mini.send.fn(client_mod.FnArgs(x=7))

    # Assert
    assert result == 15
