#!/usr/bin/env python3
"""Summarizes a run.sh output directory: median and min-max of each number per configuration.

    summarize.py <out-dir> > table.md
"""
import json
import re
import statistics
import sys
from pathlib import Path


def time_file(path):
    text = path.read_text()
    real, user, system = (float(x) for x in re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", text).groups())
    rss = int(re.search(r"(\d+)\s+maximum resident set size", text).group(1))
    out = {"wall_s": real, "user_s": user, "sys_s": system, "cpu_s": user + system, "max_rss_mb": rss / 2**20}
    instructions = re.search(r"(\d+)\s+instructions retired", text)
    if instructions:
        out["instructions_g"] = int(instructions.group(1)) / 1e9
    return out


def load(path):
    return float(re.search(r"load averages?: ([\d.]+)", path.read_text()).group(1))


def flatten(prefix, value, out):
    if isinstance(value, dict):
        for key, inner in value.items():
            flatten(f"{prefix}.{key}" if prefix else key, inner, out)
    elif isinstance(value, (int, float)) and not isinstance(value, bool):
        out[prefix] = float(value)


def cell(values, digits=0):
    if not values:
        return ""
    median = statistics.median(values)
    fmt = f"{{:.{digits}f}}"
    return f"{fmt.format(median)} ({fmt.format(min(values))}-{fmt.format(max(values))})"


def main():
    root = Path(sys.argv[1])
    for config in sorted(p for p in root.iterdir() if p.is_dir()):
        runs = sorted(config.glob("out.*"), key=lambda p: int(p.suffix[1:]))
        rows = {}
        loads, codes = [], []
        for out in runs:
            index = out.suffix
            if not (config / f"exit{index}").exists():
                continue  # still running
            codes.append((config / f"exit{index}").read_text().strip())
            loads.append(load(config / f"uptime{index}"))
            numbers = {}
            try:
                numbers.update(time_file(config / f"time{index}"))
            except AttributeError:
                continue
            # Today's binary prints `--timings` lines (`NAME  1.234 s`) and its exit code to stderr.
            for name, seconds in re.findall(r"^(\S+)\s+([\d.]+) s$", (config / f"time{index}").read_text(), re.M):
                numbers[f"gate.{name}_s"] = float(seconds)
            try:
                report = json.loads(out.read_text())
            except ValueError:
                report = {}
            flatten("", {k: v for k, v in report.items() if k != "outcomes"}, numbers)
            for position, plugin in enumerate(report.get("outcomes", [])):
                flat = {}
                flatten("", plugin, flat)
                for key, value in flat.items():
                    if key.startswith("outcome.counts."):
                        if position == 0:
                            numbers[key] = value
                        continue
                    numbers.setdefault(f"p.{key}", [])
                    numbers[f"p.{key}"].append(value)
            for key, value in numbers.items():
                if isinstance(value, list):
                    value = statistics.median(value) if "x3" not in config.name else max(value)
                rows.setdefault(key, []).append(value)
        print(f"### {config.name}\n")
        print(f"runs {len(runs)}, exit codes {' '.join(codes)}, load at start {cell(loads, 1)}\n")
        print("| number | median (min-max) |\n| --- | --- |")
        for key in sorted(rows):
            values = rows[key]
            digits = 2 if key.endswith("_s") or key.endswith("_g") else 0
            if key.startswith("outcome.counts.") or ".counts." in key:
                digits = 0
            print(f"| {key} | {cell(values, digits)} |")
        print()


main()
