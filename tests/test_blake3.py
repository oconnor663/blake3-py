import array
from binascii import unhexlify
import json
import numpy
from pathlib import Path
import subprocess
import sys

try:
    from blake3 import blake3
except ModuleNotFoundError:
    print("Run tests/build.py first.", file=sys.stderr)
    raise

HERE = Path(__file__).parent

VECTORS = json.load((HERE / "test_vectors.json").open())


def make_input(length):
    b = bytearray(length)
    for i in range(len(b)):
        b[i] = i % 251
    return b


def test_vectors():
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
        assert extended_hash_bytes == blake3(input_bytes).digest(
            length=extended_len)
        assert extended_hash_hex == blake3(input_bytes).hexdigest(
            length=extended_len)
        incremental_hash = blake3()
        incremental_hash.update(input_bytes[:input_len // 2])
        incremental_hash.update(input_bytes[input_len // 2:])
        assert hash_bytes == incremental_hash.digest()

        # keyed hash
        key = VECTORS["key"].encode()
        assert keyed_hash_bytes == blake3(input_bytes, key=key).digest()
        assert extended_keyed_hash_bytes == blake3(
            input_bytes, key=key).digest(length=extended_len)
        assert extended_keyed_hash_hex == blake3(
            input_bytes, key=key).hexdigest(length=extended_len)
        incremental_keyed_hash = blake3(key=key)
        incremental_keyed_hash.update(input_bytes[:input_len // 2])
        incremental_keyed_hash.update(input_bytes[input_len // 2:])
        assert keyed_hash_bytes == incremental_keyed_hash.digest()

        # derive key
        context = "BLAKE3 2019-12-27 16:29:52 test vectors context"
        assert derive_key_bytes == blake3(input_bytes,
                                          context=context).digest()
        assert extended_derive_key_bytes == blake3(
            input_bytes, context=context).digest(length=extended_len)
        assert extended_derive_key_hex == blake3(
            input_bytes, context=context).hexdigest(length=extended_len)
        incremental_derive_key = blake3(context=context)
        incremental_derive_key.update(input_bytes[:input_len // 2])
        incremental_derive_key.update(input_bytes[input_len // 2:])
        assert derive_key_bytes == incremental_derive_key.digest()


def test_buffer_types():
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
    assert incremental.digest() == blake3(
        b"onetwothreefourfivesixseveneight").digest()


def test_key_types():
    key = bytes([42]) * 32
    expected = blake3(b"foo", key=key).digest()
    # Check that we can use a bytearray or a memoryview to get the same result.
    assert expected == blake3(b"foo", key=bytearray(key)).digest()
    assert expected == blake3(b"foo", key=memoryview(key)).digest()


def test_short_key():
    try:
        blake3(b"foo", key=b"too short")
    except ValueError:
        pass
    else:
        assert False, "expected a key-too-short error"


def test_int_array_fails():
    try:
        # "i" represents the int type, which is larger than a char.
        blake3(array.array("i"))
    except BufferError:
        pass
    else:
        assert False, "expected a buffer error"


def test_strided_array_fails():
    unstrided = numpy.array([1, 2, 3, 4], numpy.uint8)
    strided = numpy.lib.stride_tricks.as_strided(unstrided,
                                                 shape=[2],
                                                 strides=[2])
    assert bytes(strided) == bytes([1, 3])
    # Unstrided works fine.
    blake3(unstrided)
    try:
        # But strided fails.
        blake3(strided)
    except BufferError:
        pass
    else:
        assert False, "expected a buffer error"


def test_string_fails():
    try:
        blake3("a string")
    except TypeError:
        pass
    else:
        assert False, "expected a type error"


def test_constants():
    import blake3
    assert blake3.OUT_LEN == 32
    assert blake3.KEY_LEN == 32


def test_example_dot_py():
    hello_hash = \
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    output = subprocess.run(
        [sys.executable, str(HERE / "example.py")],
        check=True,
        input=b"hello world",
        stdout=subprocess.PIPE).stdout.decode().strip()
    assert output == hello_hash


def test_xof():
    extended = blake3(b"foo").digest(length=100)

    for i in range(100):
        assert extended[:i] == blake3(b"foo").digest(length=i)
        assert extended[i:] == blake3(b"foo").digest(length=100 - i, seek=i)


def test_multithreading():
    b = make_input(10**6)
    expected = blake3(b).digest()
    assert expected == blake3(b, multithreading=True).digest()
    incremental = blake3()
    incremental.update(b, multithreading=True)
    assert expected == incremental.digest()


def test_key_context_incompatible():
    zero_key = bytearray(32)
    try:
        blake3(b"foo", key=zero_key, context="")
    except ValueError:
        pass
    else:
        assert False, "expected a type error"


def test_name():
    b = blake3()
    assert b.name == "blake3"


def test_copy_basic():
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


def test_copy_multithreading():
    """This test is somewhat redundant and takes a belt-and-suspenders approach. If the rest
    of the tests pass but this test fails, something *very* weird is going on. """
    b = make_input(10 ** 6)
    b2 = make_input(10 ** 6)
    b3 = make_input(10 ** 6)

    h1 = blake3(b, multithreading=True)
    expected = h1.digest()
    h2 = h1.copy()
    h3 = blake3(b, multithreading=True)
    assert expected == h2.digest()
    h1.update(b2, multithreading=True)
    h3.update(b2, multithreading=True)
    h3.update(b3, multithreading=True)

    expected2 = h1.digest()
    assert expected2 != h2.digest(), "Independence test failed"
    h2.update(b2, multithreading=True)
    assert expected2 == h2.digest(), "Update state of copy diverged from expected state"

    h2.update(b3, multithreading=True)
    assert h2.digest() == h3.digest(), "Update state of copy diverged from expected state"
