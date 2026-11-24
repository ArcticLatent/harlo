#!/usr/bin/env python3
"""Writes the model catalog bundled with harlo-engine: models.dev's
api.json (MIT), trimmed to what the engine reads.

    tools/models-dev-snapshot.py [api.json]

Without a file it downloads https://models.dev/api.json. The server
refreshes the same catalog at run time; this snapshot is what it starts
with when it has none."""

import json
import os
import sys
import urllib.request

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "harlo-engine", "catalog", "models.json")
PROVIDER = ("id", "name", "env", "npm", "api")
MODEL = ("id", "name", "attachment", "reasoning", "tool_call", "limit", "cost", "status")


def trim(catalog):
    out = {}
    for pid, provider in sorted(catalog.items()):
        kept = {k: provider[k] for k in PROVIDER if k in provider}
        models = {}
        for mid, model in provider.get("models", {}).items():
            m = {k: model[k] for k in MODEL if k in model}
            if "modalities" in model:
                m["input"] = model["modalities"].get("input", [])
            if "cost" in m:
                m["cost"] = {k: v for k, v in m["cost"].items()
                             if k in ("input", "output", "cache_read", "cache_write")}
            models[mid] = m
        kept["models"] = models
        out[pid] = kept
    return out


def main():
    if len(sys.argv) > 1:
        with open(sys.argv[1]) as f:
            catalog = json.load(f)
    else:
        with urllib.request.urlopen("https://models.dev/api.json", timeout=60) as r:
            catalog = json.load(r)
    with open(OUT, "w") as f:
        json.dump(trim(catalog), f, separators=(",", ":"), sort_keys=True)
        f.write("\n")
    print(f"{os.path.getsize(OUT)} bytes, {len(catalog)} providers -> {os.path.normpath(OUT)}")


if __name__ == "__main__":
    main()
