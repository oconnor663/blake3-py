from binascii import unhexlify
import json
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
    assert expected == blake3(bytearray(b"foo")).digest()
    assert expected == blake3(memoryview(b"foo")).digest()
    assert expected == blake3(memoryview(bytearray(b"foo"))).digest()

    incremental = blake3()
    incremental.update(b"foo")
    incremental.update(bytearray(b"foo"))
    incremental.update(memoryview(b"foo"))
    incremental.update(memoryview(bytearray(b"foo")))
    assert incremental.digest() == blake3(b"foofoofoofoo").digest()


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
    except TypeError:
        pass
    else:
        assert False, "expected a type error"
