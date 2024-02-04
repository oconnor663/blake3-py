from os import PathLike

__version__: str = ...

class blake3:
    name: str
    digest_size: int
    block_size: int
    key_size: int
    AUTO: int
    def __init__(
        self,
        # TODO: use collections.abc.Buffer here when PEP 688 lands in Python 3.12
        data: bytes = ...,
        /,
        *,
        key: bytes = ...,
        derive_key_context: str = ...,
        max_threads: int = ...,
        usedforsecurity: bool = ...,
    ): ...
    # TODO: use collections.abc.Buffer here when PEP 688 lands in Python 3.12
    def update(self, data: bytes, /) -> blake3: ...
    def update_mmap(self, path: str | PathLike[str]) -> blake3: ...
    def copy(self) -> blake3: ...
    def reset(self) -> None: ...
    def digest(self, length: int = ..., *, seek: int = ...) -> bytes: ...
    def hexdigest(self, length: int = ..., *, seek: int = ...) -> str: ...
