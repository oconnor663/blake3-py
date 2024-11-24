from os import PathLike
from collections.abc import Buffer

__version__: str = ...

class blake3:
    name: str
    digest_size: int
    block_size: int
    key_size: int
    AUTO: int
    def __init__(
        self,
        data: Buffer = ...,
        /,
        *,
        key: Buffer = ...,
        derive_key_context: str = ...,
        max_threads: int = ...,
        usedforsecurity: bool = ...,
    ): ...
    def update(self, data: Buffer, /) -> blake3: ...
    def update_mmap(self, path: str | PathLike[str]) -> blake3: ...
    def copy(self) -> blake3: ...
    def reset(self) -> None: ...
    def digest(self, length: int = ..., *, seek: int = ...) -> bytes: ...
    def hexdigest(self, length: int = ..., *, seek: int = ...) -> str: ...
