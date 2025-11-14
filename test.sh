#!/bin/bash

ARCH=riscv64 LOG=warn UTEST=yield SMP=1 make utest

if [ $? -ne 0 ]; then
    echo "[test script] yield test failed!"
    exit 1
fi

ARCH=riscv64 LOG=warn UTEST=wait SMP=1 make utest

if [ $? -ne 0 ]; then
    echo "[test script] wait test failed!"
    exit 1
fi

ARCH=riscv64 LOG=warn UTEST=all SMP=4 make utest

if [ $? -ne 0 ]; then
    echo "[test script] SMP test failed!"
    exit 1
fi

echo "[test script] all test passed!"
exit 0