#!/usr/bin/env python3
"""Verify environment overrides are validated by the Rust compiler."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    version = subprocess.check_output(["rustc", "+stable", "-vV"], text=True)
    host = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
    env = os.environ.copy()
    env.pop("MINECRAFT_VERSION", None)
    env.pop("MINECRAFT_PROTOCOL_VERSION", None)
    cases = [
        ({}, True),
        ({"MINECRAFT_VERSION": 'custom"version\\test\n', "MINECRAFT_PROTOCOL_VERSION": "775"}, True),
        ({"MINECRAFT_VERSION": ""}, False),
        ({"MINECRAFT_VERSION": "v" * 400}, False),
        ({"MINECRAFT_PROTOCOL_VERSION": ""}, False),
        ({"MINECRAFT_PROTOCOL_VERSION": "-"}, False),
        ({"MINECRAFT_PROTOCOL_VERSION": "2147483648"}, False),
    ]
    command = ["cargo", "+stable", "check", "-p", "microcraft-protocol", "--target", host, "--locked"]
    for overrides, should_pass in cases:
        result = subprocess.run(command, cwd=ROOT, env=env | overrides, text=True, capture_output=True)
        if (result.returncode == 0) != should_pass:
            raise RuntimeError(f"Unexpected compilation result for {overrides!r}:\n{result.stderr}")
        if not should_pass and "evaluation" not in result.stderr:
            raise RuntimeError(f"Expected a const-evaluation error, got:\n{result.stderr}")
    # Leave the normal host build usable after testing failed configurations.
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    print(f"All {len(cases)} build-time configuration checks passed.")


if __name__ == "__main__":
    main()
