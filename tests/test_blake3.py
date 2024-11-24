import array
from binascii import unhexlify
import json
import numpy
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any

from blake3 import blake3, __version__

HERE = Path(__file__).parent

VECTORS = json.load((HERE / "test_vectors.json").open())


def make_input(length: int) -> bytes:
    b = bytearray(length)
    for i in range(len(b)):
        b[i] = i % 251
    return b


def test_vectors() -> None:
    cases = VECTORS["cases"]
    for case in cases:
        input_len = int(case["input_len"])
        input_bytes = make_input(input_len)
        extended_hash_hex = case["hash"]
        extended_keyed_hash_hex = case["keyed_hash"]
        extended_derive_key_hex = case["derive_key"]
        extended_hash_bytes = unhexlify(extended_hash_hex)
        extended_keyed_hash_bytes = unhexlify(extended_keyed_hash_hex)
        extended_derive_key_bytes = unhexlify(extended_derive_key_hex)
        hash_bytes = extended_hash_bytes[:32]
        keyed_hash_bytes = extended_keyed_hash_bytes[:32]
        derive_key_bytes = extended_derive_key_bytes[:32]
        extended_len = len(extended_hash_bytes)
        assert extended_len == len(extended_keyed_hash_bytes)
        assert extended_len == len(extended_derive_key_bytes)

        # default hash
        assert hash_bytes == blake3(input_bytes).digest()
        assert extended_hash_bytes == blake3(input_bytes).digest(length=extended_len)
        assert extended_hash_hex == blake3(input_bytes).hexdigest(length=extended_len)
        incremental_hash = blake3()
        incremental_hash.update(input_bytes[: input_len // 2])
        incremental_hash.update(input_bytes[input_len // 2 :])
        assert hash_bytes == incremental_hash.digest()

        # keyed hash
        key = VECTORS["key"].encode()
        assert keyed_hash_bytes == blake3(input_bytes, key=key).digest()
        assert extended_keyed_hash_bytes == blake3(input_bytes, key=key).digest(
            length=extended_len
        )
        assert extended_keyed_hash_hex == blake3(input_bytes, key=key).hexdigest(
            length=extended_len
        )
        incremental_keyed_hash = blake3(key=key)
        incremental_keyed_hash.update(input_bytes[: input_len // 2])
        incremental_keyed_hash.update(input_bytes[input_len // 2 :])
        assert keyed_hash_bytes == incremental_keyed_hash.digest()

        # derive key
        context = "BLAKE3 2019-12-27 16:29:52 test vectors context"
        assert (
            derive_key_bytes == blake3(input_bytes, derive_key_context=context).digest()
        )
        assert extended_derive_key_bytes == blake3(
            input_bytes, derive_key_context=context
        ).digest(length=extended_len)
        assert extended_derive_key_hex == blake3(
            input_bytes, derive_key_context=context
        ).hexdigest(length=extended_len)
        incremental_derive_key = blake3(derive_key_context=context)
        incremental_derive_key.update(input_bytes[: input_len // 2])
        incremental_derive_key.update(input_bytes[input_len // 2 :])
        assert derive_key_bytes == incremental_derive_key.digest()


def test_buffer_types() -> None:
    expected = blake3(b"foo").digest()
    assert expected == blake3(memoryview(b"foo")).digest()
    assert expected == blake3(bytearray(b"foo")).digest()
    assert expected == blake3(memoryview(bytearray(b"foo"))).digest()
    # "B" means unsigned char. See https://docs.python.org/3/library/array.html.
    assert expected == blake3(array.array("B", b"foo")).digest()
    assert expected == blake3(memoryview(array.array("B", b"foo"))).digest()
    # "b" means (signed) char.
    assert expected == blake3(array.array("b", b"foo")).digest()
    assert expected == blake3(memoryview(array.array("b", b"foo"))).digest()

    incremental = blake3()
    incremental.update(b"one")
    incremental.update(memoryview(b"two"))
    incremental.update(bytearray(b"three"))
    incremental.update(memoryview(bytearray(b"four")))
    incremental.update(array.array("B", b"five"))
    incremental.update(memoryview(array.array("B", b"six")))
    incremental.update(array.array("b", b"seven"))
    incremental.update(memoryview(array.array("b", b"eight")))
    assert incremental.digest() == blake3(b"onetwothreefourfivesixseveneight").digest()


def test_key_types() -> None:
    key = bytes([42]) * 32
    expected = blake3(b"foo", key=key).digest()
    # Check that we can use a bytearray or a memoryview to get the same result.
    assert expected == blake3(b"foo", key=bytearray(key)).digest()
    assert expected == blake3(b"foo", key=memoryview(key)).digest()


def test_invalid_key_lengths() -> None:
    for key_length in range(0, 100):
        key = b"\xff" * key_length
        if key_length == blake3.key_size:
            # This length works without throwing.
            blake3(b"foo", key=key)
        else:
            # Other lengths throw.
            try:
                blake3(b"foo", key=key)
                assert False, "should throw"
            except ValueError:
                pass


def test_int_array_fails() -> None:
    try:
        # "i" represents the int type, which is larger than a char.
        blake3(array.array("i"))
    # We get BufferError in Rust and ValueError in C.
    except (BufferError, ValueError):
        pass
    else:
        assert False, "expected a buffer error"

    # The same thing, but with the update method.
    try:
        blake3().update(array.array("i"))
    except (BufferError, ValueError):
        pass
    else:
        assert False, "expected a buffer error"


def test_strided_array_fails() -> None:
    unstrided = numpy.array([1, 2, 3, 4], numpy.uint8)
    strided = numpy.lib.stride_tricks.as_strided(unstrided, shape=[2], strides=[2])
    assert bytes(strided) == bytes([1, 3])
    # Unstrided works fine.
    blake3(unstrided)
    try:
        # But strided fails.
        blake3(strided)
    # We get BufferError in Rust and ValueError in C.
    except (BufferError, ValueError):
        pass
    else:
        assert False, "expected a buffer error"


def test_string_fails() -> None:
    try:
        blake3("a string")  # type: ignore
    except TypeError:
        pass
    else:
        assert False, "expected a type error"


def test_constants() -> None:
    # These are class attributes, so they should work on the class itself and
    # also on instances of the class.
    assert blake3.name == "blake3"
    assert blake3.digest_size == 32
    assert blake3.block_size == 64
    assert blake3.key_size == 32
    assert blake3.AUTO == -1
    assert blake3().name == "blake3"
    assert blake3().digest_size == 32
    assert blake3().block_size == 64
    assert blake3().key_size == 32
    assert blake3().AUTO == -1


def test_example_dot_py() -> None:
    hello_hash = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    output = (
        subprocess.run(
            [sys.executable, str(HERE / "example.py")],
            check=True,
            input=b"hello world",
            stdout=subprocess.PIPE,
        )
        .stdout.decode()
        .strip()
    )
    assert output == hello_hash


def test_xof() -> None:
    extended = blake3(b"foo").digest(length=100)

    for i in range(100):
        assert extended[:i] == blake3(b"foo").digest(length=i)
        assert extended[i:] == blake3(b"foo").digest(length=100 - i, seek=i)


def test_max_threads_value() -> None:
    b = make_input(10**6)
    expected = blake3(b).digest()
    assert expected == blake3(b, max_threads=2).digest()
    incremental = blake3()
    incremental.update(b)
    assert expected == incremental.digest()


def test_max_threads_auto() -> None:
    b = make_input(10**6)
    expected = blake3(b).digest()
    assert expected == blake3(b, max_threads=blake3.AUTO).digest()
    incremental = blake3()
    incremental.update(b)
    assert expected == incremental.digest()


def test_key_context_incompatible() -> None:
    zero_key = bytearray(32)
    try:
        blake3(b"foo", key=zero_key, derive_key_context="")
    except ValueError:
        pass
    else:
        assert False, "expected a type error"


def test_name() -> None:
    b = blake3()
    assert b.name == "blake3"


def test_copy_basic() -> None:
    b = make_input(10**6)
    b2 = make_input(10**6)
    h1 = blake3(b)
    expected = h1.digest()
    h2 = h1.copy()
    assert expected == h2.digest()
    h1.update(b2)
    expected2 = h1.digest()
    assert expected2 != h2.digest(), "Independence test failed"
    h2.update(b2)
    assert expected2 == h2.digest(), "Update state of copy diverged from expected state"


def test_copy_with_threads() -> None:
    """This test is somewhat redundant and takes a belt-and-suspenders approach. If the rest
    of the tests pass but this test fails, something *very* weird is going on."""
    b = make_input(10**6)
    b2 = make_input(10**6)
    b3 = make_input(10**6)

    h1 = blake3(b, max_threads=2)
    expected = h1.digest()
    h2 = h1.copy()
    h3 = blake3(b, max_threads=2)
    assert expected == h2.digest()
    h1.update(b2)
    h3.update(b2)
    h3.update(b3)

    expected2 = h1.digest()
    assert expected2 != h2.digest(), "Independence test failed"
    h2.update(b2)
    assert expected2 == h2.digest(), "Update state of copy diverged from expected state"

    h2.update(b3)
    assert (
        h2.digest() == h3.digest()
    ), "Update state of copy diverged from expected state"


def test_version() -> None:
    # Just sanity check that it's a version string. Don't assert the specific
    # version, both because we don't want to bother with parsing Cargo.toml,
    # and because these tests might be reused to test C bindings.
    assert type(__version__) is str
    assert len(__version__.split(".")) == 3


def test_invalid_max_threads() -> None:
    # Check 0.
    try:
        blake3(max_threads=0)
    except ValueError:
        pass
    else:
        assert False, "expected a ValueError"

    # -1 is AUTO, so skip that and check -2.
    try:
        blake3(max_threads=-2)
    except ValueError:
        pass
    else:
        assert False, "expected a ValueError"


def test_positional_only_arguments() -> None:
    try:
        # Passing the data as a keyword argument should fail.
        blake3(data=b"")  # type: ignore
        assert False, "expected TypeError"
    except TypeError:
        pass
    try:
        # Passing the data as a keyword argument should fail.
        blake3().update(data=b"")  # type: ignore
        assert False, "expected TypeError"
    except TypeError:
        pass


def test_keyword_only_arguments() -> None:
    try:
        # Passing the key as a positional argument should fail.
        blake3(b"", b"\0" * 32)  # type: ignore
        assert False, "expected TypeError"
    except TypeError:
        pass

    # The digest length is allowed to be positional or keyword.
    blake3(b"").digest(32)
    blake3(b"").digest(length=32)
    blake3(b"").hexdigest(32)
    blake3(b"").hexdigest(length=32)
    # But the seek parameter is keyword-only.
    blake3(b"").digest(32, seek=0)
    blake3(b"").digest(length=32, seek=0)
    blake3(b"").hexdigest(32, seek=0)
    blake3(b"").hexdigest(length=32, seek=0)
    try:
        blake3(b"").digest(32, 0)  # type: ignore
        assert False, "expected TypeError"
    except TypeError:
        pass
    try:
        blake3(b"").hexdigest(32, 0)  # type: ignore
        assert False, "expected TypeError"
    except TypeError:
        pass


def test_usedforsecurity_ignored() -> None:
    blake3(usedforsecurity=True)
    blake3(usedforsecurity=False)


def test_context_must_be_str() -> None:
    # string works
    blake3(derive_key_context="foo")
    try:
        # bytes fails
        blake3(derive_key_context=b"foo")  # type: ignore
        assert False, "should fail"
    except TypeError:
        pass


def test_buffers_released() -> None:
    key = bytearray(32)
    message = bytearray(32)

    # These operations acquire 3 different Py_Buffer handles. We're testing
    # that they get released properly.
    hasher = blake3(message, key=key)
    hasher.update(message)

    # These extensions will fail if a buffer isn't properly released.
    key.extend(b"foo")
    message.extend(b"foo")


def test_reset() -> None:
    hasher = blake3()
    hash1 = hasher.digest()
    hasher.update(b"foo")
    hash2 = hasher.digest()
    hasher.reset()
    hash3 = hasher.digest()
    hasher.update(b"foo")
    hash4 = hasher.digest()

    assert hash1 != hash2
    assert hash1 == hash3
    assert hash2 == hash4


def test_output_overflows_isize() -> None:
    try:
        blake3().digest(sys.maxsize + 1)
        assert False, "should throw"
    except (OverflowError, MemoryError):
        pass
    try:
        blake3().hexdigest((sys.maxsize // 2) + 1)
        assert False, "should throw"
    except (OverflowError, MemoryError):
        pass


# Currently the canonical path of the Rust implementation is
# `blake3.blake3.blake3`, while the canonical path of the C implementation is
# `blake3.blake3`. Both implementations should pass this test. See also:
# https://github.com/mkdocstrings/mkdocstrings/issues/451 and
# https://github.com/PyO3/maturin/discussions/1365
def test_module_name() -> None:
    global_scope: dict[str, Any] = {}
    exec(f"from {blake3.__module__} import blake3 as foobar", global_scope)
    assert global_scope["foobar"] is blake3


def test_mmap() -> None:
    input_bytes = bytes([42]) * 1_000_000
    # Note that we can't use NamedTemporaryFile here, because we can't open it
    # again on Windows.
    (fd, temp_path) = tempfile.mkstemp()
    os.close(fd)
    with open(temp_path, "wb") as f:
        f.write(input_bytes)

    # Test all three threading modes, and both str and Path arguments. Note
    # that PyO3 doesn't support converting Python bytes to a Rust PathBuf,
    # I think because that's not generally possible on Windows.
    hasher1 = blake3()
    hasher1.update_mmap(temp_path)
    assert blake3(input_bytes).digest() == hasher1.digest()

    hasher2 = blake3(max_threads=blake3.AUTO)
    hasher2.update_mmap(Path(temp_path))
    assert blake3(input_bytes).digest() == hasher2.digest()

    # Also test that update and update_mmap return self.
    hasher3 = (
        blake3(max_threads=4)
        .update(input_bytes)
        .update_mmap(temp_path)
        .update_mmap(path=Path(temp_path))
    )
    assert blake3(3 * input_bytes).digest() == hasher3.digest()

    # Test a nonexistent file.
    try:
        hasher3.update_mmap("/non/existent/file.txt")
        assert False, "expected a file not found error"
    except FileNotFoundError:
        pass
