#! /usr/bin/env bash

set -e -u -o pipefail

set -x

FLAGS="-O3 -Wall -fPIC -Ivendor -I/usr/include/python3.10"

# Assembly

# gcc -shared $FLAGS \
#     blake3module.c \
#     vendor/blake3.c \
#     vendor/blake3_dispatch.c \
#     vendor/blake3_portable.c \
#     vendor/blake3_sse2_x86-64_unix.S \
#     vendor/blake3_sse41_x86-64_unix.S \
#     vendor/blake3_avx2_x86-64_unix.S \
#     vendor/blake3_avx512_x86-64_unix.S \
#     -o blake3.so

# Intrinsics

gcc -c $FLAGS -msse2 vendor/blake3_sse2.c -o blake3_sse2.o
gcc -c $FLAGS -msse4.1 vendor/blake3_sse41.c -o blake3_sse41.o
gcc -c $FLAGS -mavx2 vendor/blake3_avx2.c -o blake3_avx2.o
gcc -c $FLAGS -mavx512f -mavx512vl vendor/blake3_avx512.c -o blake3_avx512.o
gcc -shared $FLAGS \
    blake3module.c \
    vendor/blake3.c \
    vendor/blake3_dispatch.c \
    vendor/blake3_portable.c \
    blake3_sse2.o \
    blake3_sse41.o \
    blake3_avx2.o \
    blake3_avx512.o \
    -o blake3.so
