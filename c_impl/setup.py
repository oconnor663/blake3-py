from setuptools import setup, Extension

extension = Extension("spam", sources=["spammodule.c"])

setup(
    name="spam",
    version="1.0",
    description="SPAAAAAM",
    ext_modules=[extension],
)
