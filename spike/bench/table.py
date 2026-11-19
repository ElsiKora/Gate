#!/usr/bin/env python3
"""The RESULTS.md tables from a run.sh output directory: per configuration the median and min-max of
instructions retired, CPU, wall and peak memory, and the phase clocks the workload reports.

    table.py <out-dir> [<out-dir> ...]

Configurations of the same name in several directories are pooled.
"""
import json
import re
import statistics
import sys
from pathlib import Path


def runs(config):
    out = []
    for time_file in sorted(config.glob("time.*"), key=lambda p: int(p.suffix[1:])):
        index = time_file.suffix
        if not (config / f"exit{index}").exists():
            continue
        text = time_file.read_text()
        match = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", text)
        if not match:
            continue
        real, user, system = (float(x) for x in match.groups())
        run = {
            "wall": real,
            "cpu": user + system,
            "rss": int(re.search(r"(\d+)\s+maximum resident set size", text).group(1)) / 2**20,
            "load": float(re.search(r"load averages?: ([\d.]+)", (config / f"uptime{index}").read_text()).group(1)),
            "exit": (config / f"exit{index}").read_text().strip(),
        }
        instructions = re.search(r"(\d+)\s+instructions retired", text)
        if instructions:
            run["ins"] = int(instructions.group(1)) / 1e9
        for name, seconds in re.findall(r"^(\S+)\s+([\d.]+) s$", text, re.M):
            run[f"gate.{name}"] = float(seconds)
        try:
            report = json.loads((config / f"out{index}").read_text())
        except ValueError:
            report = {}
        run["report"] = report
        out.append(run)
    return out


def med(values):
    return statistics.median(values) if values else None


def cell(values, digits):
    if not values:
        return ""
    fmt = f"{{:.{digits}f}}"
    return f"{fmt.format(med(values))} ({fmt.format(min(values))}-{fmt.format(max(values))})"


def main():
    data = {}
    for root in map(Path, sys.argv[1:]):
        for config in sorted(p for p in root.iterdir() if p.is_dir()):
            data.setdefault(config.name, []).extend(runs(config))
    data = dict(sorted(data.items()))
    print("| configuration | runs (exit) | load | G instructions | CPU s | wall s | max RSS MB |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for name, items in data.items():
        if not items:
            continue
        exits = " ".join(sorted({r["exit"] for r in items}))
        print(
            f"| {name} | {len(items)} ({exits}) | {cell([r['load'] for r in items], 0)} "
            f"| {cell([r['ins'] for r in items if 'ins' in r], 2)} | {cell([r['cpu'] for r in items], 2)} "
            f"| {cell([r['wall'] for r in items], 2)} | {cell([r['rss'] for r in items], 0)} |"
        )
    # Instructions retired grow with machine load and vary by about 0.5 G between runs, more than
    # one rule costs, so per-rule costs come from the workload's own phase clocks (milliseconds,
    # first plugin of a run), not from differences between rows.
    phases = ["parse.read", "parse.oxc", "walk", "semantic", "resolve"]
    print()
    print("Phase wall time inside the plugin, median ms (min-max):")
    print()
    print("| configuration | " + " | ".join(phases) + " |")
    print("| --- |" + " --- |" * len(phases))
    fastest = {}
    for name, items in data.items():
        outcomes = [r["report"]["outcomes"][0]["outcome"]["phases"] for r in items if r["report"].get("outcomes")]
        if not outcomes:
            continue
        # Under load the fastest run is the least disturbed one: compare minimums.
        fastest[name] = {ph: min([o[ph] for o in outcomes if ph in o], default=None) for ph in phases}
        cells = [cell([o[ph] for o in outcomes if ph in o], 0) for ph in phases]
        print(f"| {name} | " + " | ".join(cells) + " |")
    pairs = [(n, "wasm-host" + n[len("native"):]) for n in fastest if n.startswith("native-")]
    pairs += [(n, n.replace("native", "wasm")) for n in fastest if n.startswith("hook-native")]
    pairs = [(a, b) for a, b in pairs if b in fastest]
    if pairs:
        print()
        print("Wasm over native, ratio of the phase minimums (the least disturbed run of each):")
        print()
        print("| pair | " + " | ".join(phases) + " |")
        print("| --- |" + " --- |" * len(phases))
        for a, b in pairs:
            ratios = []
            for ph in phases:
                x, y = fastest[a].get(ph), fastest[b].get(ph)
                ratios.append(f"{y / x:.2f}" if x and y else "")
            print(f"| {b} / {a} | " + " | ".join(ratios) + " |")

main()
