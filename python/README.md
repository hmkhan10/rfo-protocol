# RFO Python SDK

Native Python bindings for the RFO Protocol engine.

## Install

```bash
pip install rfo-core
```

Or from source:

```bash
cd rfo
cargo build --release
cd python
pip install -e .
```

## Quickstart

```python
from rfo import OptResolver

resolver = OptResolver()
resolver.register("mysite.opt", core_file_dict)
core = resolver.resolve("mysite.opt")
print(core["site"]["title"])
```

## Requirements

- Python 3.9+
- `librfo_core.so` / `.dylib` / `.dll` on `LD_LIBRARY_PATH`
