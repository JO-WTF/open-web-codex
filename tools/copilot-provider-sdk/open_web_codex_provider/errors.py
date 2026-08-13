"""Stable typed failures returned by provider primitives."""


class ProviderContractError(ValueError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


class WorkspaceFileError(ValueError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)
